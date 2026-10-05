//! Retains the original relation scope until child completion and accepted finish preparation.

use std::future::Future;

use salsa::execution_probe::{RunError, RunResult};

use super::guard_control::{SourceGuardControl, guard_result};
use super::{BorrowedPairs, PairChildren, PairEffects, RelationSourceEffects};
use crate::types::Type;
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::PreparedCycleFinish;
use crate::types::relation::guard::{
    PendingCachedRelation, RelationGuardEffects, RelationGuardStep, RelationKey, RelationScope,
};
use crate::types::relation::{TypeRelation, TypeRelationChecker};

#[cfg(test)]
mod tests;

impl<
    'run,
    'db: 'run + 'c,
    'a,
    'c: 'a,
    E: RelationSourceEffects<'run, 'db>,
    P: PairChildren<'run, 'db, 'c>,
> RelationGuardEffects<'a, 'c, 'db> for BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;
    type Prepared =
        PreparedCycleFinish<'a, 'db, TypeRelation, RelationKey<'db>, ConstraintSet<'db, 'c>, 1>;

    async fn start(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<RelationGuardStep<'a, 'c, 'db>> {
        let lookup = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if !std::ptr::eq(checker.constraints, self.constraints) {
                    return Err(RunError::Contract(
                        "relation guard changed its constraint builder",
                    ));
                }
                Ok(RelationGuardStep::start_admitted(
                    self.db,
                    checker,
                    source,
                    target,
                    &mut SourceGuardControl {
                        endpoint: self.endpoint,
                    },
                ))
            })
            .await;
        guard_result(lookup, self.effects).await
    }

    async fn complete(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                if !std::ptr::eq(checker.constraints, self.constraints) {
                    return Err(RunError::Contract(
                        "relation guard changed its constraint builder",
                    ));
                }
                result.verify_builder(self.constraints);
                self.endpoint.check_completion()?;
                Ok(result)
            })
            .await)
    }

    async fn child<F>(&self, work: impl FnOnce() -> F) -> RunResult<ConstraintSet<'db, 'c>>
    where
        F: Future<Output = RunResult<ConstraintSet<'db, 'c>>>,
    {
        Ok(self.endpoint.child_call(work).await)
    }

    async fn prepare_finish(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        scope: &RelationScope<'a, 'c, 'db>,
        result: ConstraintSet<'db, 'c>,
    ) -> RunResult<Self::Prepared> {
        #[cfg(test)]
        observations::before_prepare(self.db, checker.relation_visitor.ownership_probe_counts().0);
        let prepared = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                if !std::ptr::eq(checker.constraints, self.constraints) {
                    return Err(RunError::Contract(
                        "relation guard changed its constraint builder",
                    ));
                }
                result.verify_builder(self.constraints);
                let prepared = scope.prepare_finish_admitted(
                    &result,
                    &mut SourceGuardControl {
                        endpoint: self.endpoint,
                    },
                );
                #[cfg(test)]
                if prepared.is_ok() {
                    observations::after_prepare(self.db);
                }
                Ok(prepared)
            })
            .await;
        guard_result(prepared, self.effects).await
    }

    async fn commit_finish(
        &self,
        mut scope: RelationScope<'a, 'c, 'db>,
        prepared: Self::Prepared,
        result: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        match scope.commit_prepared_admitted(prepared, result, ConstraintSet::has_same_identity) {
            Ok(result) => Ok(result),
            Err(_) => Ok(self
                .endpoint
                .local_call(|| {
                    Err(RunError::Contract(
                        "relation guard finish preparation became stale",
                    ))
                })
                .await),
        }
    }

    async fn is_never_satisfied(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.satisfy(constraints, false).await
    }

    async fn resume(
        &self,
        pending: PendingCachedRelation<'a, 'c, 'db>,
        is_never_satisfied: bool,
    ) -> RunResult<RelationGuardStep<'a, 'c, 'db>> {
        let resumed = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(pending.resume_admitted(
                    self.db,
                    is_never_satisfied,
                    &mut SourceGuardControl {
                        endpoint: self.endpoint,
                    },
                ))
            })
            .await;
        guard_result(resumed, self.effects).await
    }

    async fn recursive_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        PairEffects::recursive_type_pair_fallback(self, checker, source, target).await
    }
}

#[cfg(test)]
pub(in crate::types) mod observations {
    use std::cell::Cell;

    use crate::Db;

    thread_local! {
        static ENTERED: Cell<usize> = const { Cell::new(0) };
        static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static ACTIVE: Cell<usize> = const { Cell::new(0) };
        static CANCEL: Cell<bool> = const { Cell::new(false) };
        static CANCEL_PREPARED: Cell<bool> = const { Cell::new(false) };
        static PREPARED: Cell<usize> = const { Cell::new(0) };
    }

    pub(in crate::types) fn reset(cancel: bool) {
        ENTERED.set(0);
        REMAINING.set(None);
        ACTIVE.set(0);
        CANCEL.set(cancel);
        CANCEL_PREPARED.set(false);
        PREPARED.set(0);
    }

    pub(in crate::types) fn progress() -> (usize, Option<usize>, usize) {
        (ENTERED.get(), REMAINING.get(), ACTIVE.get())
    }

    pub(super) fn before_prepare(db: &dyn Db, active: usize) {
        let entered = ENTERED.get();
        ENTERED.set(entered + 1);
        if entered == 0 {
            REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                db,
            ));
            ACTIVE.set(active);
        }
        if CANCEL.replace(false) {
            db.cancellation_token().cancel();
        }
    }

    pub(super) fn cancel_prepared() {
        CANCEL_PREPARED.set(true);
    }

    pub(super) fn prepared_count() -> usize {
        PREPARED.get()
    }

    pub(super) fn after_prepare(db: &dyn Db) {
        PREPARED.set(PREPARED.get() + 1);
        if CANCEL_PREPARED.replace(false) {
            db.cancellation_token().cancel();
        }
    }
}
