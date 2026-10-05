//! Callable-source comparisons borrow their caller's checker through conversion and diagnostics.

use salsa::execution_probe::{BorrowOrCopy, RunError, RunResult};

use super::{BorrowedPairs, PairChildren, RelationSourceEffects, RelationSourceOperation};
use crate::types::callable::{CallableType, CallableTypeKind, CallableTypes};
use crate::types::constraints::ConstraintSet;
use crate::types::relation::callable::{
    CallableSourceEffects, CallableSourceFacts, finish_callable_source_with,
    prepare_callable_target_with,
};
use crate::types::relation::{TypeRelation, TypeRelationChecker};
use crate::types::{Type, UpcastPolicy};

pub(super) struct BorrowedCallableSource<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P>
    BorrowedCallableSource<'pairs, 'effects, 'run, 'db, 'a, 'c, E, P>
{
    pub(super) fn new(
        pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
        checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
    ) -> Self {
        Self { pairs, checker }
    }
}

impl<'run, 'db: 'run + 'c, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    CallableSourceEffects<'c, 'db> for BorrowedCallableSource<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;

    async fn prepare_target(
        &self,
        source: Type<'db>,
        target: CallableType<'db>,
        relation: TypeRelation,
    ) -> RunResult<CallableType<'db>> {
        prepare_callable_target_with(source, target, relation, CallableSourceFacts, self).await
    }

    async fn convert(
        &self,
        source: Type<'db>,
        policy: UpcastPolicy,
    ) -> RunResult<Option<CallableTypes<'db>>> {
        #[cfg(test)]
        crate::types::cyclic::guard_storage::observations::relation_conversion(
            std::ptr::from_ref(self.checker.relation_visitor).addr(),
            self.checker.relation_visitor.ownership_probe_counts(),
        );
        self.pairs
            .effects
            .callable_conversion(self.checker.env, source, policy)
            .await
    }

    async fn finish_comparison(
        &self,
        source: Type<'db>,
        target: CallableType<'db>,
        callables: Option<CallableTypes<'db>>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        finish_callable_source_with(source, target, callables, self).await
    }

    async fn is_function_like(&self, target: CallableType<'db>) -> RunResult<bool> {
        let kind = self
            .pairs
            .endpoint
            .read_field(
                target
                    .field_requests(self.pairs.endpoint.field_request_context())
                    .kind(),
                &BorrowOrCopy,
            )
            .await;
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                self.pairs.endpoint.check_completion()?;
                Ok(kind == CallableTypeKind::FunctionLike)
            })
            .await)
    }

    async fn regularize(&self, _target: CallableType<'db>) -> RunResult<CallableType<'db>> {
        self.pairs
            .effects
            .unavailable(RelationSourceOperation::CallableSourceRegularization)
            .await
    }

    async fn never(&self) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                self.pairs.endpoint.check_completion()?;
                Ok(self.checker.never())
            })
            .await)
    }

    async fn compare(
        &self,
        callables: &CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .compare_callables(self.checker, callables, target)
            .await
    }

    async fn has_context(&self) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                self.pairs.endpoint.check_completion()?;
                Ok(self.checker.report_context().is_some())
            })
            .await)
    }

    async fn should_provide_context(&self, source: Type<'db>) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(4)?;
                self.pairs.endpoint.check_completion()?;
                Ok(self.checker.should_provide_callable_upcast_context(source))
            })
            .await)
    }

    async fn is_never_satisfied(&self, result: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        self.pairs.satisfy(result, false).await
    }

    async fn report_context(
        &self,
        _source: Type<'db>,
        _callables: &CallableTypes<'db>,
    ) -> RunResult<()> {
        self.pairs
            .effects
            .unavailable(RelationSourceOperation::CallableSourceContext)
            .await
    }
}
