//! Compares a source type to a callable after resolving the source's call signatures.

pub(in crate::types) mod live_signature_effects;

#[cfg(test)]
mod scheduled_effects;

use std::convert::Infallible;

#[cfg(test)]
use super::TypeVarEvaluation;
use super::{TypeRelation, TypeRelationChecker};
use crate::Db;
use crate::types::callable::{CallableConversionRequest, CallableType, CallableTypes};
use crate::types::constraints::ConstraintSet;
#[cfg(test)]
use crate::types::cyclic::{CycleDetectorScope, CycleDetectorVisit};
use crate::types::{ErrorContext, Type, UpcastPolicy};

#[cfg(test)]
mod future_ownership_probe;

#[cfg(test)]
mod scheduled_probe;

#[cfg(test)]
pub(in crate::types) mod scheduled_requests;

pub(in crate::types) struct CallableSourceFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousCallableSourceEffects)]
    pub(in crate::types) trait CallableSourceEffects<'c, 'db: 'c> {
        type Error;

        #[operation(child)]
        async fn prepare_target(&self, source: Type<'db>, target: CallableType<'db>, relation: TypeRelation) -> Result<CallableType<'db>, Self::Error>;
        #[operation(child)]
        async fn convert(&self, source: Type<'db>, policy: UpcastPolicy) -> Result<Option<CallableTypes<'db>>, Self::Error>;
        #[operation(child)]
        async fn finish_comparison(&self, source: Type<'db>, target: CallableType<'db>, callables: Option<CallableTypes<'db>>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(source)]
        async fn is_function_like(&self, target: CallableType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn regularize(&self, target: CallableType<'db>) -> Result<CallableType<'db>, Self::Error>;
        #[operation(local)]
        async fn never(&self) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn compare(&self, callables: &CallableTypes<'db>, target: CallableType<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn has_context(&self) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn should_provide_context(&self, source: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_never_satisfied(&self, result: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_context(&self, source: Type<'db>, callables: &CallableTypes<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl CallableSourceFacts {
        fn may_regularize(&self, source: Type<'_>, relation: TypeRelation) -> bool {
            relation.is_assignability() && matches!(source, Type::BoundMethod(_))
        }

        fn policy(&self, relation: TypeRelation) -> UpcastPolicy {
            UpcastPolicy::from(relation)
        }
    }

    #[synchronous(prepare_callable_target_sync)]
    #[capabilities(effects = CallableSourceEffects, facts = CallableSourceFacts)]
    #[passive_values()]
    pub(in crate::types) async fn prepare_callable_target_with<'c, 'db: 'c, E: CallableSourceEffects<'c, 'db>>(
        source: Type<'db>, target: CallableType<'db>, relation: TypeRelation,
        facts: CallableSourceFacts, effects: &E,
    ) -> Result<CallableType<'db>, E::Error> {
        // Bound methods can be assigned to inferred function-like callback types,
        // but are not nominal subtypes of functions.
        if facts.may_regularize(source, relation) && effects.is_function_like(target).await? {
            effects.regularize(target).await
        } else {
            Ok(target)
        }
    }

    #[synchronous(finish_callable_source_sync)]
    #[capabilities(effects = CallableSourceEffects)]
    #[passive_values()]
    pub(in crate::types) async fn finish_callable_source_with<'c, 'db: 'c, E: CallableSourceEffects<'c, 'db>>(
        source: Type<'db>, target: CallableType<'db>, callables: Option<CallableTypes<'db>>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let Some(callables) = callables else { return effects.never().await; };
        let result = effects.compare(&callables, target).await?;
        if effects.has_context().await?
            && effects.should_provide_context(source).await?
            && effects.is_never_satisfied(result).await?
        {
            effects.report_context(source, &callables).await?;
        }
        Ok(result)
    }

    /// Resolves and compares a callable source while its caller retains the relation visit.
    /// Conversion can reenter relation checks, so the visit stays active through comparison.
    /// Failed conversion yields an unsatisfied relation. Refusal returns an error so the caller
    /// abandons the visit without caching the comparison.
    #[synchronous(check_callable_source_sync)]
    #[capabilities(effects = CallableSourceEffects, facts = CallableSourceFacts)]
    #[passive_values()]
    pub(in crate::types) async fn check_callable_source_with<'c, 'db: 'c, E: CallableSourceEffects<'c, 'db>>(
        source: Type<'db>, target: CallableType<'db>, relation: TypeRelation,
        facts: CallableSourceFacts, effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let target = effects.prepare_target(source, target, relation).await?;
        let callables = effects.convert(source, facts.policy(relation)).await?;
        effects.finish_comparison(source, target, callables).await
    }
}

struct InlineCallableSource<'db, 'check, 'a, 'c> {
    db: &'db dyn Db,
    checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'c, 'db: 'c> SynchronousCallableSourceEffects<'c, 'db>
    for InlineCallableSource<'db, '_, '_, 'c>
{
    type Error = Infallible;

    fn prepare_target(
        &self,
        source: Type<'db>,
        target: CallableType<'db>,
        relation: TypeRelation,
    ) -> Result<CallableType<'db>, Infallible> {
        prepare_callable_target_sync(source, target, relation, CallableSourceFacts, self)
    }

    fn convert(
        &self,
        source: Type<'db>,
        policy: UpcastPolicy,
    ) -> Result<Option<CallableTypes<'db>>, Infallible> {
        Ok(
            CallableConversionRequest::new(source, policy).evaluate(
                self.db,
                self.checker.env,
                None,
            ),
        )
    }

    fn finish_comparison(
        &self,
        source: Type<'db>,
        target: CallableType<'db>,
        callables: Option<CallableTypes<'db>>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        finish_callable_source_sync(source, target, callables, self)
    }

    fn is_function_like(&self, target: CallableType<'db>) -> Result<bool, Infallible> {
        Ok(target.is_function_like(self.db))
    }

    fn regularize(&self, target: CallableType<'db>) -> Result<CallableType<'db>, Infallible> {
        Ok(target.into_regular(self.db))
    }

    fn never(&self) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.never())
    }

    fn compare(
        &self,
        callables: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self
            .checker
            .check_callables_vs_callable(self.db, callables, target))
    }

    fn has_context(&self) -> Result<bool, Infallible> {
        Ok(self.checker.report_context().is_some())
    }

    fn should_provide_context(&self, source: Type<'db>) -> Result<bool, Infallible> {
        Ok(self.checker.should_provide_callable_upcast_context(source))
    }

    fn is_never_satisfied(&self, result: ConstraintSet<'db, 'c>) -> Result<bool, Infallible> {
        Ok(result.is_never_satisfied(self.db, self.checker.env))
    }

    fn report_context(
        &self,
        source: Type<'db>,
        callables: &CallableTypes<'db>,
    ) -> Result<(), Infallible> {
        if let Some(context) = self.checker.report_context() {
            context.push(ErrorContext::InferredCallableType {
                source,
                callable: callables.to_type(self.db, self.checker.env),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
type RelationScope<'a, 'c, 'db> = CycleDetectorScope<
    'a,
    'db,
    TypeRelation,
    (Type<'db>, Type<'db>, TypeRelation, TypeVarEvaluation),
    ConstraintSet<'db, 'c>,
    1,
>;

#[cfg(test)]
enum CallableRelationStep<'checker, 'a, 'c, 'db> {
    Convert(PendingCallableConversion<'checker, 'a, 'c, 'db>),
    Complete(ConstraintSet<'db, 'c>),
}

#[cfg(test)]
impl<'checker, 'a, 'c, 'db> CallableRelationStep<'checker, 'a, 'c, 'db> {
    fn start(
        db: &'db dyn Db,
        checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: CallableType<'db>,
    ) -> Self {
        let collect_context = checker.is_context_collection_enabled();
        let visit = match checker.relation_visitor.try_begin_visit(
            db,
            (
                source,
                Type::Callable(target),
                checker.relation,
                checker.typevar_evaluation,
            ),
            |result| !collect_context || !result.is_never_satisfied(db, checker.env),
        ) {
            CycleDetectorVisit::Ready(result) => return Self::Complete(result),
            CycleDetectorVisit::Cycle(item) => {
                return Self::Complete(checker.recursive_type_pair_fallback(db, item.0, item.1));
            }
            CycleDetectorVisit::Pending(visit) => visit,
        };

        let target = match prepare_callable_target_sync(
            source,
            target,
            checker.relation,
            CallableSourceFacts,
            &InlineCallableSource { db, checker },
        ) {
            Ok(target) => target,
            Err(never) => match never {},
        };

        Self::Convert(PendingCallableConversion {
            request: CallableConversionRequest::new(source, UpcastPolicy::from(checker.relation)),
            checker,
            source,
            target,
            visit,
        })
    }
}

#[cfg(test)]
struct PendingCallableConversion<'checker, 'a, 'c, 'db> {
    request: CallableConversionRequest<'db>,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    source: Type<'db>,
    target: CallableType<'db>,
    // Conversion can reenter relations. Keep the original visit active through both
    // conversion and signature comparison, including diagnostic recomputation.
    visit: RelationScope<'a, 'c, 'db>,
}

#[cfg(test)]
impl<'c, 'db> PendingCallableConversion<'_, '_, 'c, 'db> {
    fn resume(
        self,
        db: &'db dyn Db,
        callables: Option<CallableTypes<'db>>,
    ) -> ConstraintSet<'db, 'c> {
        let result = match finish_callable_source_sync(
            self.source,
            self.target,
            callables,
            &InlineCallableSource {
                db,
                checker: self.checker,
            },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        };
        self.visit.finish(result)
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_callable_source(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: CallableType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, source, Type::Callable(target), || {
            match check_callable_source_sync(
                source,
                target,
                self.relation,
                CallableSourceFacts,
                &InlineCallableSource { db, checker: self },
            ) {
                Ok(result) => result,
                Err(never) => match never {},
            }
        })
    }
}
