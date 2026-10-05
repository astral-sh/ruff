//! Source-intersection effects retain the borrowed checker through member and expansion comparisons.

use std::ops::ControlFlow;

use salsa::execution_probe::{RunError, RunResult};

use super::retained::PairChildren;
use super::{BorrowedPairs, RelationSourceEffects, RelationSourceOperation};
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::relation::TypeRelationChecker;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::source_intersection::{
    SourceIntersectionEffects, SourceIntersectionElements, should_expand_source_intersection_with,
};
use crate::types::{BoundTypeVarInstance, IntersectionType, NewType, Type};

pub(super) struct BorrowedSourceIntersection<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P>
    BorrowedSourceIntersection<'pairs, 'effects, 'run, 'db, 'a, 'c, E, P>
{
    pub(super) fn new(
        pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
        checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
    ) -> Self {
        Self { pairs, checker }
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    BorrowedSourceIntersection<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    async fn elements(
        &self,
        intersection: IntersectionType<'db>,
        implicit_object: bool,
    ) -> RunResult<SourceIntersectionElements<'db>> {
        let elements = self
            .pairs
            .effects
            .intersection_positive_elements(intersection)
            .await?;
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs
                    .endpoint
                    .admit_work(size_of::<SourceIntersectionElements<'db>>() * 2)?;
                Ok(SourceIntersectionElements::new(elements, implicit_object))
            })
            .await)
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    SourceIntersectionEffects<'c, 'db>
    for BorrowedSourceIntersection<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;
    type Elements<'state>
        = SourceIntersectionElements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;
    type Context<'state>
        = bool
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

    async fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .children
            .pair(
                self.pairs.db,
                self.pairs.effects,
                self.checker,
                source,
                target,
            )
            .await
    }

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Self::Elements<'_>> {
        self.elements(intersection, false).await
    }

    async fn positive_elements_or_object(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Self::Elements<'_>> {
        self.elements(intersection, true).await
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> RunResult<Option<Type<'db>>> {
        let next = self
            .pairs
            .effects
            .intersection_next_element(&mut elements.elements)
            .await?;
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(elements.after_next(next))
            })
            .await)
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

    async fn never(&self) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(ConstraintSet::from_bool(self.checker.constraints, false))
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

    async fn should_expand(&self, intersection: IntersectionType<'db>) -> RunResult<bool> {
        should_expand_source_intersection_with(intersection, self).await
    }

    async fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        self.pairs.typevar_is_inferable(self.checker, typevar).await
    }

    async fn newtype_concrete_base(&self, newtype: NewType<'db>) -> RunResult<Type<'db>> {
        self.pairs
            .newtype_concrete_base(self.checker, newtype)
            .await
    }

    async fn expand_intersection(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Type<'db>> {
        self.pairs
            .effects
            .intersection_expand(self.checker.env, intersection)
            .await
    }

    async fn context_start(&self) -> RunResult<Self::Context<'_>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(self.checker.report_context().is_some())
            })
            .await)
    }

    async fn capture_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
    ) -> RunResult<()> {
        let enabled = self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(*context)
            })
            .await;
        if enabled {
            return self
                .pairs
                .unavailable(RelationSourceOperation::SourceIntersectionContext)
                .await;
        }
        Ok(())
    }

    async fn has_collected_context<'state>(
        &'state self,
        context: &Self::Context<'state>,
    ) -> RunResult<bool> {
        let enabled = self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(*context)
            })
            .await;
        if enabled {
            return self
                .pairs
                .unavailable(RelationSourceOperation::SourceIntersectionContext)
                .await;
        }
        Ok(false)
    }

    async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        self.pairs.satisfy(value, false).await
    }

    async fn report_context<'state>(
        &'state self,
        _context: &mut Self::Context<'state>,
        _intersection: IntersectionType<'db>,
        _target: Type<'db>,
    ) -> RunResult<()> {
        self.pairs
            .unavailable(RelationSourceOperation::SourceIntersectionContext)
            .await
    }
}
