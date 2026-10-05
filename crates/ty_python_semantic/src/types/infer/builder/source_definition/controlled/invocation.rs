//! Supplies admitted owners, argument matching, and checking for synthetic calls.

use salsa::execution_probe::{RunError, RunResult};

use super::callable_guard::GuardedPreparationEffects;
use super::{SourceAccess, SourceEffects};
use crate::types::call::bind::source_check;
#[cfg(test)]
use crate::types::call::bind::constructor_matching::{ConstructorMatchingEffects, ConstructorMatchingOperation};
use crate::types::call::invocation::{InvocationContext, InvocationEffects, invoke_with};
use crate::types::call::preparation::{BindingPreparationFacts, bindings_with};
use crate::types::call::{Bindings, CallArguments, CallError, CallErrorKind};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::relation::source::RelationSourceEffects;
use crate::types::relation::source::resources::RelationResourceAccess;
use crate::types::{Type, TypeContext};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn positional_synthetic_call(
        &self,
        env: &ProgramEnvironment<'db>,
        callable: Type<'db>,
        arguments: &[Type<'db>],
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        self.environment_program(env).await?;
        self.allocate_future(|| async {
            let count = arguments.len();
            let work = Self::checked(count.checked_mul(4).and_then(|work| work.checked_add(4)))?;
            let bytes = Self::checked(
                CallArguments::capacity_bytes(count)
                    .and_then(|payload| payload.checked_add(size_of::<CallArguments<'_, 'db>>())),
            )?;
            let mut argument_owner: Option<CallArguments<'_, 'db>> =
                self.initialize_value(|| None).await?;
            self.local(work, bytes, || {
                argument_owner = Some(CallArguments::positional(arguments.iter().copied()));
            })
            .await?;
            let arguments = argument_owner.as_ref().ok_or(RunError::Contract(
                "positional call arguments were not constructed",
            ))?;
            self.allocate_future(|| self.synthetic_call(env, callable, arguments, recursion_guard))
                .await?
                .await
        })
        .await?
        .await
    }

    /// Checks a synthetic call using already-inferred argument types.
    ///
    /// A semantic call error retains its bindings; an execution interruption returns an outer error.
    /// The invocation's continuation is admitted before it is constructed. A supplied recursion
    /// guard remains shared with the caller; otherwise the invocation retains its own guard.
    /// Guarded preparation requires the selected guard to have storage admission state; an
    /// untracked supplied guard interrupts entry with `CallableGuardOperation::StorageOrigin`.
    pub(in crate::types::infer) async fn synthetic_call(
        &self,
        env: &ProgramEnvironment<'db>,
        callable: Type<'db>,
        arguments: &CallArguments<'_, 'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        self.environment_program(env).await?;
        self.allocate_future(|| {
            invoke_with(
                callable,
                InvocationContext {
                    db: self.db(),
                    env,
                    arguments,
                },
                recursion_guard,
                self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InvocationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Constraints = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;

    async fn new_guard(&self) -> RunResult<CallableRecursionGuard<'db>> {
        self.admitted_callable_guard().await
    }

    async fn prepare(
        &self,
        context: InvocationContext<'_, 'db>,
        callable: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<Bindings<'db>> {
        let effects = GuardedPreparationEffects {
            source: self,
            guard,
        };
        bindings_with(
            context.db,
            context.env,
            callable,
            false,
            BindingPreparationFacts,
            &effects,
        )
        .await
    }

    async fn preparation_stopped(&self, _db: &'db dyn Db) -> RunResult<bool> {
        // The native constructor probe can stop ordinary preparation. Controlled interruption
        // instead returns an outer error from the dependency that could not finish.
        self.local(1, 0, || false).await
    }

    async fn constraints(&self) -> RunResult<Self::Constraints> {
        self.resources().invocation_builder(self.endpoint()).await
    }

    async fn match_parameters(
        &self,
        context: InvocationContext<'_, 'db>,
        mut bindings: Bindings<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.constructor_matching_child(|| {
            bindings.match_parameters_with(context.db, context.env, context.arguments, self)
        }).await?;
        #[cfg(test)]
        self.before_matching(ConstructorMatchingOperation::ResultTransfer);
        self.local_with_fixed_transfers(1, 0, || {
            #[cfg(test)]
            self.after_matching(ConstructorMatchingOperation::ResultTransfer);
            bindings
        }).await
    }

    async fn check_types(
        &self,
        context: InvocationContext<'_, 'db>,
        constraints: &Self::Constraints,
        bindings: &mut Bindings<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<Result<(), CallErrorKind>> {
        source_check::check(
            context.db,
            context.env,
            *constraints,
            context.arguments,
            bindings,
            TypeContext::default(),
            &[],
            Some(guard),
            self,
        )
        .await
    }

    async fn semantic_error(
        &self,
        kind: CallErrorKind,
        bindings: Bindings<'db>,
    ) -> RunResult<CallError<'db>> {
        // Moving the bindings retains their existing payloads; this operation allocates only
        // the outer box.
        let carrier_bytes = Self::checked(
            size_of::<Option<Bindings<'db>>>().checked_add(size_of::<Option<CallError<'db>>>()),
        )?;
        self.local(2, carrier_bytes, || ()).await?;
        let mut input = Some(bindings);
        let mut output = None;
        let action = || -> RunResult<()> {
            let bindings = input
                .take()
                .ok_or(RunError::Contract("call-error bindings already consumed"))?;
            output = Some(CallError(kind, Box::new(bindings)));
            Ok(())
        };
        let bytes = Self::checked(
            size_of::<Bindings<'db>>()
                .checked_add(size_of::<CallError<'db>>())
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<CallError<'db>>>()))
                .and_then(|bytes| bytes.checked_add(size_of_val(&action)))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<()>>())),
        )?;
        self.local(7, bytes, action).await??;
        output.ok_or(RunError::Contract("call error was not constructed"))
    }
}
