//! Binder comparisons share the execution stack while retaining their invocation's builder.

use std::future::Future;

use salsa::execution_probe::FieldRequest;

use super::checking_effects::{BindingsEffects, CheckContext};
use super::constructor::ConstructorBinding;
use super::effects::{self, BinderEffects, BinderLegacyEffect};
use super::{
    ArgumentTypeChecker, BinderComparison, BinderCondition, Bindings, CallArguments, CheckTypesMode,
};
use crate::types::Type;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::function::OverloadLiteral;
use crate::types::relation::execution::TaskEndpoint;
use crate::types::relation::execution::resources::CallRelationOwners;
use crate::types::relation::{EquivalenceChecker, TypeRelationChecker};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Satisfaction {
    Always,
    Never,
}

/// Each returned future is one child operation. Implementations must supervise recursive work
/// within that operation; putting a synchronous recursive call in a future does not do that.
pub(in crate::types) trait ConditionExecutor<'run, 'c, 'db> {
    type Error: 'run;

    fn context_mismatch(&self) -> Self::Error;

    fn relate(
        &'run self,
        checker: TypeRelationChecker<'run, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> + 'run;

    fn equivalent(
        &'run self,
        checker: EquivalenceChecker<'run, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> + 'run;

    fn satisfaction(
        &'run self,
        constraints: ConstraintSet<'db, 'c>,
        test: Satisfaction,
    ) -> impl Future<Output = Result<bool, Self::Error>> + 'run;
}

pub(super) struct BinderConditions<'run, 'env, 'c, 'db, X: ConditionExecutor<'run, 'c, 'db>> {
    pub(super) endpoint: TaskEndpoint<'run, X::Error>,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    pub(super) owners: &'run CallRelationOwners<'env, 'c, 'db>,
    pub(super) executor: &'run X,
}

impl<'run, 'env: 'run, 'c: 'run, 'db: 'run, X: ConditionExecutor<'run, 'c, 'db>>
    BinderConditions<'run, 'env, 'c, 'db, X>
{
    async fn evaluate(
        &self,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        condition: BinderCondition<'db>,
    ) -> Result<bool, X::Error> {
        if !std::ptr::eq(env, self.env) || !std::ptr::eq(constraints, self.constraints) {
            return Err(self.executor.context_mismatch());
        }
        // A condition is a new relation root. Descendant comparisons retain this bundle through
        // their checker; separate conditions can have different inferable variables.
        let owners = self
            .owners
            .allocate(&self.endpoint, self.env, self.constraints)?;
        let executor = self.executor;
        let (comparison, test) = match condition {
            BinderCondition::Always(comparison) => (comparison, Satisfaction::Always),
            BinderCondition::Never(comparison) => (comparison, Satisfaction::Never),
        };
        let result = match comparison {
            BinderComparison::Assignable {
                source,
                target,
                inferable_typevars,
            } => {
                let checker = owners.assignability(inferable_typevars);
                self.endpoint
                    .demand(move || executor.relate(checker, source, target))?
                    .await?
            }
            BinderComparison::Equivalent { left, right } => {
                let checker = owners.equivalence();
                self.endpoint
                    .demand(move || executor.equivalent(checker, left, right))?
                    .await?
            }
        };
        self.endpoint
            .demand(move || executor.satisfaction(result, test))?
            .await
    }
}

/// Conditions use typed tasks. The remaining effects retain an explicit provider so constructor,
/// native, inference and expansion operations cannot inherit an unrestricted default.
pub(super) struct ScheduledBinder<'run, 'env, 'c, 'db, X, P>
where
    X: ConditionExecutor<'run, 'c, 'db>,
{
    pub(super) conditions: BinderConditions<'run, 'env, 'c, 'db, X>,
    pub(super) other: &'run P,
}

impl<'run, 'c, 'db, X: ConditionExecutor<'run, 'c, 'db>, P> effects::sealed::Sealed
    for ScheduledBinder<'run, '_, 'c, 'db, X, P>
{
}

impl<'run, 'env: 'run, 'c: 'run, 'db: 'run, X, P> BinderEffects<'db>
    for ScheduledBinder<'run, 'env, 'c, 'db, X, P>
where
    X: ConditionExecutor<'run, 'c, 'db>,
    P: BinderEffects<'db, Error = X::Error>,
{
    type Error = X::Error;

    fn recursion_guard(&self) -> Option<&CallableRecursionGuard<'db>> {
        self.other.recursion_guard()
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        self.other.field(request).await
    }

    async fn function_overloads(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error> {
        self.other.function_overloads(db, last_definition).await
    }

    fn legacy<T>(
        &self,
        effect: BinderLegacyEffect,
        operation: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        self.other.legacy(effect, operation)
    }

    fn inspect_argument_expansions(
        &self,
        arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> Result<bool, Self::Error> {
        self.other.inspect_argument_expansions(arguments, inspect)
    }

    async fn condition(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        condition: BinderCondition<'db>,
    ) -> Result<bool, Self::Error> {
        self.conditions.evaluate(env, constraints, condition).await
    }

    fn defer_typevartuple_check(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.other
            .defer_typevartuple_check(checker, declared, expected, argument)
    }

    fn trace_span(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        signature: Type<'db>,
    ) -> tracing::Span {
        self.other.trace_span(db, env, arguments, signature)
    }
}

impl<'run, 'env: 'run, 'c: 'run, 'db: 'run, X, P> BindingsEffects<'db>
    for ScheduledBinder<'run, 'env, 'c, 'db, X, P>
where
    X: ConditionExecutor<'run, 'c, 'db>,
    P: BindingsEffects<'db, Error = X::Error>,
{
    async fn constructor(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
        mode: CheckTypesMode,
    ) -> Result<(), Self::Error> {
        self.other.constructor(binding, context, mode).await
    }

    async fn known_cases(
        &self,
        bindings: &mut Bindings<'db>,
        context: CheckContext<'_, 'db>,
    ) -> Result<(), Self::Error> {
        self.other.known_cases(bindings, context).await
    }

    async fn downstream(
        &self,
        binding: &mut ConstructorBinding<'db>,
        context: CheckContext<'_, 'db>,
    ) -> Result<(), Self::Error> {
        self.other.downstream(binding, context).await
    }

    fn step<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.other.step(operation)
    }
}
