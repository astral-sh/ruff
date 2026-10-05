//! Compares a source type to a callable after resolving the source's call signatures.

use super::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::Db;
use crate::types::callable::{CallableConversionRequest, CallableType, CallableTypes};
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::{CycleDetectorScope, CycleDetectorVisit};
use crate::types::{ErrorContext, Type, UpcastPolicy};

type RelationScope<'a, 'c, 'db> = CycleDetectorScope<
    'a,
    'db,
    TypeRelation,
    (Type<'db>, Type<'db>, TypeRelation, TypeVarEvaluation),
    ConstraintSet<'db, 'c>,
    1,
>;

enum CallableRelationStep<'checker, 'a, 'c, 'db> {
    Convert(PendingCallableConversion<'checker, 'a, 'c, 'db>),
    Complete(ConstraintSet<'db, 'c>),
}

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

        // Bound methods can be assigned to inferred function-like callback types,
        // but are not nominal subtypes of functions.
        let target = if checker.relation.is_assignability()
            && matches!(source, Type::BoundMethod(_))
            && target.is_function_like(db)
        {
            target.into_regular(db)
        } else {
            target
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

struct PendingCallableConversion<'checker, 'a, 'c, 'db> {
    request: CallableConversionRequest<'db>,
    checker: &'checker TypeRelationChecker<'a, 'c, 'db>,
    source: Type<'db>,
    target: CallableType<'db>,
    // Conversion can reenter relations. Keep the original visit active through both
    // conversion and signature comparison, including diagnostic recomputation.
    visit: RelationScope<'a, 'c, 'db>,
}

impl<'c, 'db> PendingCallableConversion<'_, '_, 'c, 'db> {
    fn resume(
        self,
        db: &'db dyn Db,
        callables: Option<CallableTypes<'db>>,
    ) -> ConstraintSet<'db, 'c> {
        let Some(callables) = callables else {
            return self.visit.finish(self.checker.never());
        };

        let result = self
            .checker
            .check_callables_vs_callable(db, &callables, self.target);

        if let Some(context) = self.checker.report_context()
            && self
                .checker
                .should_provide_callable_upcast_context(self.source)
            && result.is_never_satisfied(db, self.checker.env)
        {
            context.push(ErrorContext::InferredCallableType {
                source: self.source,
                callable: callables.to_type(db, self.checker.env),
            });
        }

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
        match CallableRelationStep::start(db, self, source, target) {
            CallableRelationStep::Complete(result) => result,
            CallableRelationStep::Convert(pending) => {
                let callables = pending.request.evaluate(db, self.env, None);
                pending.resume(db, callables)
            }
        }
    }
}
