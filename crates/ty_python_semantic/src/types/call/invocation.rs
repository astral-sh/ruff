//! Checks calls whose argument types have already been inferred.
//!
//! One recursion guard is retained through binding preparation and checking. Preparation's
//! active expansion scopes end before parameter matching, as they do for ordinary `Type::try_call`.
//! A semantic call failure retains its bindings; an execution interruption remains a separate error.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::bind::CheckTypesMode;
use super::{Bindings, CallArguments, CallError, CallErrorKind};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::{Type, TypeContext};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(in crate::types) struct InvocationContext<'a, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'a ProgramEnvironment<'db>,
    pub(in crate::types) arguments: &'a CallArguments<'a, 'db>,
}

pub(in crate::types) struct OrdinaryInvocationEffects;

shared_semantic_family! {
    #[synchronous(SynchronousInvocationEffects)]
    pub(in crate::types) trait InvocationEffects<'db> {
        type Error;
        type Constraints;

        #[operation(local)]
        async fn new_guard(&self) -> Result<CallableRecursionGuard<'db>, Self::Error>;
        #[operation(child)]
        async fn prepare(&self, context: InvocationContext<'_, 'db>, callable: Type<'db>, guard: &CallableRecursionGuard<'db>) -> Result<Bindings<'db>, Self::Error>;
        #[operation(local)]
        async fn preparation_stopped(&self, db: &'db dyn Db) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn constraints(&self) -> Result<Self::Constraints, Self::Error>;
        #[operation(child)]
        async fn match_parameters(&self, context: InvocationContext<'_, 'db>, bindings: Bindings<'db>) -> Result<Bindings<'db>, Self::Error>;
        #[operation(child)]
        async fn check_types(&self, context: InvocationContext<'_, 'db>, constraints: &Self::Constraints, bindings: &mut Bindings<'db>, guard: &CallableRecursionGuard<'db>) -> Result<Result<(), CallErrorKind>, Self::Error>;
        #[operation(local)]
        async fn semantic_error(&self, kind: CallErrorKind, bindings: Bindings<'db>) -> Result<CallError<'db>, Self::Error>;
    }

    /// Matches and checks already-inferred arguments against a callable's bindings.
    ///
    /// The inner result retains bindings for both successful calls and semantic call errors.
    /// The outer error interrupts the invocation before it can publish a semantic result.
    #[synchronous(invoke_sync)]
    #[capabilities(effects = InvocationEffects)]
    #[passive_values(Err)]
    pub(in crate::types) async fn invoke_with<'db, E: InvocationEffects<'db>>(
        callable: Type<'db>,
        context: InvocationContext<'_, 'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
        effects: &E,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, E::Error> {
        let guard = match recursion_guard {
            Some(guard) => guard,
            None => &effects.new_guard().await?,
        };
        let bindings = effects.prepare(context, callable, guard).await?;
        if effects.preparation_stopped(context.db).await? {
            return Ok(Ok(bindings));
        }
        let constraints = effects.constraints().await?;
        let mut bindings = effects.match_parameters(context, bindings).await?;
        match effects.check_types(context, &constraints, &mut bindings, guard).await? {
            Ok(()) => Ok(Ok(bindings)),
            Err(kind) => Ok(Err(effects.semantic_error(kind, bindings).await?)),
        }
    }
}

impl<'db> SynchronousInvocationEffects<'db> for OrdinaryInvocationEffects {
    type Error = Infallible;
    type Constraints = ConstraintSetBuilder<'db>;

    fn new_guard(&self) -> Result<CallableRecursionGuard<'db>, Self::Error> {
        Ok(CallableRecursionGuard::new())
    }

    fn prepare(
        &self,
        context: InvocationContext<'_, 'db>,
        callable: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(callable.bindings_impl(context.db, context.env, guard))
    }

    fn preparation_stopped(&self, db: &'db dyn Db) -> Result<bool, Self::Error> {
        #[cfg(test)]
        let stopped = crate::types::constructor::expansion_probe::stopped(db);
        #[cfg(not(test))]
        let stopped = {
            let _ = db;
            false
        };
        Ok(stopped)
    }

    fn constraints(&self) -> Result<Self::Constraints, Self::Error> {
        Ok(ConstraintSetBuilder::new())
    }

    fn match_parameters(
        &self,
        context: InvocationContext<'_, 'db>,
        bindings: Bindings<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(bindings.match_parameters(context.db, context.env, context.arguments))
    }

    fn check_types(
        &self,
        context: InvocationContext<'_, 'db>,
        constraints: &Self::Constraints,
        bindings: &mut Bindings<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<Result<(), CallErrorKind>, Self::Error> {
        Ok(bindings.check_types_impl_with_recursion_guard(
            context.db,
            context.env,
            constraints,
            context.arguments,
            TypeContext::default(),
            &[],
            CheckTypesMode::Finalize,
            Some(guard),
        ))
    }

    fn semantic_error(
        &self,
        kind: CallErrorKind,
        bindings: Bindings<'db>,
    ) -> Result<CallError<'db>, Self::Error> {
        Ok(CallError(kind, Box::new(bindings)))
    }
}
