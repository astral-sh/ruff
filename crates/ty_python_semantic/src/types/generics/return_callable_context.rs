//! Preserve unspecialized declarations when renaming variables in returned callables.

use std::convert::Infallible;

use super::context_construction::ContextVariables;
use crate::types::mapping::return_callables::ReturnTypevarReplacements;
use crate::types::typevar::BoundTypeVarIdentity;
use crate::types::{BoundTypeVarInstance, GenericContext};
use crate::{Db, FxOrderSet, ProgramEnvironment};

/// Supplies ordered context storage and the identity reads needed to preserve declarations.
pub(in crate::types) trait ReturnCallableContextEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error>;

    async fn lookup(
        &self,
        replacements: ReturnTypevarReplacements<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;

    async fn identity(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;

    async fn new_variables(
        &self,
        capacity: usize,
    ) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error>;

    async fn insert(
        &self,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error>;

    async fn finish(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Self::Error>;
}

/// Removes declarations mapped to a different identity, preserving the remaining order.
/// Unmatched variables and replacements with the same [`BoundTypeVarIdentity`] retain the
/// original declaration. The return-scoping caller separately adds the renamed declarations
/// to each mapped callable signature's generic context.
pub(in crate::types) async fn map_return_callable_context_with<
    'db,
    E: ReturnCallableContextEffects<'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    context: GenericContext<'db>,
    replacements: ReturnTypevarReplacements<'_, 'db>,
    effects: &E,
) -> Result<GenericContext<'db>, E::Error> {
    let source = effects.variables(db, context).await?;
    let count = effects.local(Some(1), Some(0), || source.len()).await?;
    let mut kept = effects.new_variables(count).await?;
    let mut index = effects.local(Some(1), Some(0), || 0usize).await?;
    loop {
        let variable = effects
            .local(
                Some(3),
                Some(size_of::<(
                    &BoundTypeVarIdentity<'db>,
                    &BoundTypeVarInstance<'db>,
                )>()),
                || {
                    let variable = source.get_index(index).map(|(_, variable)| *variable);
                    if variable.is_some() {
                        index += 1;
                    }
                    variable
                },
            )
            .await?;
        let Some(variable) = variable else { break };
        let retain = match effects.lookup(replacements, variable).await? {
            None => true,
            Some(replacement) => {
                let replacement = effects.identity(db, replacement).await?;
                let original = effects.identity(db, variable).await?;
                effects
                    .local(Some(4), Some(0), || replacement == original)
                    .await?
            }
        };
        if retain {
            effects.insert(&mut kept, variable).await?;
        }
    }
    effects.finish(db, env, kept).await
}

/// Executes the same declaration filter through ordinary database and context operations.
#[derive(Debug)]
pub(in crate::types) struct InlineReturnCallableContext;

impl<'db> ReturnCallableContextEffects<'db> for InlineReturnCallableContext {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_inner(db))
    }

    async fn lookup(
        &self,
        replacements: ReturnTypevarReplacements<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(replacements.get(variable))
    }

    async fn identity(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(variable.identity(db))
    }

    async fn new_variables(
        &self,
        capacity: usize,
    ) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(FxOrderSet::with_capacity_and_hasher(
            capacity,
            Default::default(),
        ))
    }

    async fn insert(
        &self,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        variables.insert(variable);
        Ok(())
    }

    async fn finish(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(GenericContext::from_typevar_instances(db, env, variables))
    }
}
