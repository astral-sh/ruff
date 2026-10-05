use std::ops::ControlFlow;

use salsa::execution_probe::{RunError, RunResult};

use super::retained::PairChildren;
use super::{BorrowedPairs, RelationSourceEffects, disjoint_guard};
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::relation::DisjointnessChecker;
use crate::types::relation::disjoint_intersection::{
    DisjointIntersectionEffects, DisjointIntersectionOperands,
    check_disjoint_intersection_structural_with,
};
use crate::types::relation::pair_effects::PairEffects;
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::{IntersectionType, Type};

pub(super) struct BorrowedDisjointIntersection<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    checker: &'pairs DisjointnessChecker<'a, 'c, 'db>,
}

impl<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P>
    BorrowedDisjointIntersection<'pairs, 'effects, 'run, 'db, 'a, 'c, E, P>
{
    pub(super) fn new(
        pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
        checker: &'pairs DisjointnessChecker<'a, 'c, 'db>,
    ) -> Self {
        Self { pairs, checker }
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    DisjointIntersectionEffects<'c, 'db>
    for BorrowedDisjointIntersection<'_, '_, 'run, 'db, '_, 'c, E, P>
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

    async fn finite_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.pairs
            .effects
            .intersection_alternatives(self.checker.env, intersection)
            .await
    }

    async fn disjoint_pair(
        &self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .children
            .disjoint_pair(self.pairs.db, self.pairs.effects, self.checker, left, right)
            .await
    }

    async fn guarded_structural(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        disjoint_guard::with_guard(
            self.pairs.db,
            self.checker,
            left,
            right,
            self.pairs.effects,
            || check_disjoint_intersection_structural_with(left, right, operands, self),
        )
        .await
    }

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

    async fn subtyping_pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .children
            .derived_subtyping_pair(
                self.pairs.db,
                self.pairs.effects,
                self.checker,
                source,
                target,
            )
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
                    ConstraintFoldKind::Any,
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

    async fn is_trivially_always_satisfied(
        &self,
        value: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                value.verify_builder(self.checker.constraints);
                Ok(value.is_trivially_always_satisfied())
            })
            .await)
    }

    async fn disjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .combine_constraints(
                self.checker.constraints,
                ConstraintFoldKind::Any,
                left,
                right,
            )
            .await
    }
}
