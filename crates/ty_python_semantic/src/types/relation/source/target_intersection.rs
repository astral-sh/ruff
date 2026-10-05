//! Target-intersection effects retain the borrowed relation checker across both folds.

use std::ops::ControlFlow;

use salsa::execution_probe::{RunError, RunResult};

use super::retained::PairChildren;
use super::{BorrowedPairs, RelationSourceEffects, RelationSourceOperation};
use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::mapping::effects::{
    MappingDispatchFacts, MaterializationEffects, materialization_with,
};
use crate::types::relation::TypeRelationChecker;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::target_intersection::TargetIntersectionEffects;
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::{IntersectionType, MaterializationKind, NominalInstanceType, Type};

#[cfg(test)]
pub(in crate::types) mod materialization_observations {
    use std::cell::Cell;

    use crate::Db;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(in crate::types) struct Snapshot {
        pub(in crate::types) count: usize,
        pub(in crate::types) before: [Option<usize>; 8],
        pub(in crate::types) after: [Option<usize>; 8],
    }

    thread_local! {
        static OBSERVATIONS: Cell<Snapshot> = const { Cell::new(Snapshot {
            count: 0,
            before: [None; 8],
            after: [None; 8],
        }) };
        static CANCEL_AT: Cell<Option<usize>> = const { Cell::new(None) };
    }

    pub(in crate::types) fn reset(cancel_at: Option<usize>) {
        OBSERVATIONS.set(Snapshot {
            count: 0,
            before: [None; 8],
            after: [None; 8],
        });
        CANCEL_AT.set(cancel_at);
    }

    pub(in crate::types) fn snapshot() -> Snapshot {
        OBSERVATIONS.get()
    }

    pub(super) fn enter(db: &dyn Db) -> usize {
        let mut snapshot = OBSERVATIONS.get();
        let index = snapshot.count;
        if let Some(slot) = snapshot.before.get_mut(index) {
            *slot = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        snapshot.count += 1;
        OBSERVATIONS.set(snapshot);
        if CANCEL_AT.get() == Some(snapshot.count) {
            CANCEL_AT.set(None);
            db.cancellation_token().cancel();
        }
        index
    }

    pub(super) fn finish(db: &dyn Db, index: usize) {
        let mut snapshot = OBSERVATIONS.get();
        if let Some(slot) = snapshot.after.get_mut(index) {
            *slot = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        OBSERVATIONS.set(snapshot);
    }
}

pub(super) struct BorrowedTargetIntersection<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P>
    BorrowedTargetIntersection<'pairs, 'effects, 'run, 'db, 'a, 'c, E, P>
{
    pub(super) fn new(
        pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
        checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
    ) -> Self {
        Self { pairs, checker }
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    MaterializationEffects<'db> for BorrowedTargetIntersection<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Failure = RunError;

    async fn nominal_is_generic(
        &self,
        _db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        self.pairs
            .effects
            .nominal_is_definition_generic(instance)
            .await
    }

    async fn cached_materialization(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        self.pairs
            .effects
            .cached_materialization(self.checker.env, ty, kind)
            .await
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    TargetIntersectionEffects<'c, 'db>
    for BorrowedTargetIntersection<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;
    type Elements<'state>
        = Elements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Self::Elements<'_>> {
        self.pairs
            .effects
            .intersection_positive_elements(intersection)
            .await
    }

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Self::Elements<'_>> {
        self.pairs
            .effects
            .intersection_negative_elements(intersection)
            .await
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> RunResult<Option<Type<'db>>> {
        self.pairs.effects.intersection_next_element(elements).await
    }

    async fn positive_pair(
        &self,
        source: Type<'db>,
        positive: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .check_type_pair(self.checker, source, positive)
            .await
    }

    async fn has_context(&self) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(self.checker.report_context().is_some())
            })
            .await)
    }

    async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        self.pairs.satisfy(value, false).await
    }

    async fn report_context(
        &self,
        _source: Type<'db>,
        _positive: Type<'db>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<()> {
        self.pairs
            .unavailable(RelationSourceOperation::TargetIntersectionContext)
            .await
    }

    async fn fold_start(&self) -> RunResult<Self::Fold<'_>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs
                    .endpoint
                    .admit_work(size_of::<ConstraintFold<'db, 'c>>() * 2)?;
                Ok(ConstraintFold::new(
                    self.checker.constraints,
                    ConstraintFoldKind::All,
                ))
            })
            .await)
    }

    async fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        self.pairs.push_constraints(fold, next).await
    }

    async fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs.finish_constraints(fold).await
    }

    async fn is_trivially_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                value.verify_builder(self.checker.constraints);
                Ok(value.is_trivially_never_satisfied())
            })
            .await)
    }

    async fn bottom_materialization(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        #[cfg(test)]
        let observation = materialization_observations::enter(self.pairs.db);
        self.pairs
            .endpoint
            .local_call(|| self.pairs.endpoint.admit_work(2))
            .await;
        let result = materialization_with(
            self.pairs.db,
            ty,
            MaterializationKind::Bottom,
            self,
            MappingDispatchFacts,
        )
        .await?;
        #[cfg(test)]
        materialization_observations::finish(self.pairs.db, observation);
        Ok(result)
    }

    async fn disjoint_pair(
        &self,
        source: Type<'db>,
        negative: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .children
            .derived_disjoint_pair(
                self.pairs.db,
                self.pairs.effects,
                self.checker,
                source,
                negative,
            )
            .await
    }

    async fn conjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .combine_constraints(
                self.checker.constraints,
                ConstraintFoldKind::All,
                left,
                right,
            )
            .await
    }
}
