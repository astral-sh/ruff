//! Target-union effects retain the borrowed checker and use admitted reads and constraint folds.

#[cfg(test)]
mod tests;

use std::ops::ControlFlow;
use std::slice;

use salsa::execution_probe::{RunError, RunResult};

use super::retained::PairChildren;
use super::source_intersection::BorrowedSourceIntersection;
use super::{BorrowedPairs, RelationSourceEffects, RelationSourceOperation};
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::relation::TypeRelationChecker;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::source_intersection::SourceIntersectionEffects;
use crate::types::relation::target_union::{TargetUnionEffects, union_has_aliases_with};
use crate::types::{
    BoundTypeVarInstance, IntersectionType, NewType, Type, TypeVarBoundOrConstraints, UnionType,
};

pub(super) struct BorrowedTargetUnion<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P>
    BorrowedTargetUnion<'pairs, 'effects, 'run, 'db, 'a, 'c, E, P>
{
    pub(super) fn new(
        pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
        checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
    ) -> Self {
        Self { pairs, checker }
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    BorrowedTargetUnion<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    pub(super) async fn has_aliases(&self, union: UnionType<'db>) -> RunResult<bool> {
        union_has_aliases_with(union, self).await
    }

    pub(super) async fn contains(&self, union: UnionType<'db>, ty: Type<'db>) -> RunResult<bool> {
        let mut elements = self.elements(union).await?;
        while let Some(element) = self.next_element(&mut elements).await? {
            if self
                .pairs
                .endpoint
                .local_call(|| {
                    self.pairs.endpoint.admit_work(2)?;
                    let work = element
                        .inline_payload_bytes()
                        .checked_add(ty.inline_payload_bytes())
                        .and_then(|work| work.checked_add(2))
                        .ok_or(RunError::Contract(
                            "target-union membership quotation overflow",
                        ))?;
                    self.pairs.endpoint.admit_work(work)?;
                    self.pairs.endpoint.check_completion()?;
                    Ok(element == ty)
                })
                .await
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    TargetUnionEffects<'c, 'db> for BorrowedTargetUnion<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;
    type Elements<'state>
        = slice::Iter<'db, Type<'db>>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;
    type Context<'state>
        = ()
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
            .check_type_pair(self.checker, source, target)
            .await
    }

    async fn elements(&self, union: UnionType<'db>) -> RunResult<Self::Elements<'_>> {
        let elements = self.pairs.effects.union_elements(union).await?;
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs
                    .endpoint
                    .admit_work(size_of::<slice::Iter<'db, Type<'db>>>() * 2)?;
                Ok(elements.iter())
            })
            .await)
    }

    async fn element_count<'state>(
        &'state self,
        elements: &Self::Elements<'state>,
    ) -> RunResult<usize> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(elements.len())
            })
            .await)
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> RunResult<Option<Type<'db>>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(
                    size_of::<Type<'db>>() + size_of::<slice::Iter<'db, Type<'db>>>() + 1,
                )?;
                Ok(elements.next().copied())
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

    async fn element_is_alias_like(&self, element: Type<'db>) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(element.is_alias_like())
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

    async fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        self.pairs.typevar_is_inferable(self.checker, typevar).await
    }

    async fn typevar_bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.pairs
            .typevar_bound_or_constraints(self.checker, typevar)
            .await
    }

    async fn source_typevar_bounds(
        &self,
        bounds: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .check_source_typevar_bounds(self.checker, bounds, target)
            .await
    }

    async fn should_expand(&self, intersection: IntersectionType<'db>) -> RunResult<bool> {
        BorrowedSourceIntersection::new(self.pairs, self.checker)
            .should_expand(intersection)
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

    async fn newtype_concrete_base(&self, newtype: NewType<'db>) -> RunResult<Type<'db>> {
        self.pairs
            .newtype_concrete_base(self.checker, newtype)
            .await
    }

    async fn context_start(&self) -> RunResult<Self::Context<'_>> {
        let enabled = self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(self.checker.report_context().is_some())
            })
            .await;
        if enabled {
            return self
                .pairs
                .unavailable(RelationSourceOperation::TargetUnionContext)
                .await;
        }
        Ok(())
    }

    async fn capture_context<'state>(
        &'state self,
        _context: &mut Self::Context<'state>,
    ) -> RunResult<()> {
        Ok(())
    }

    async fn has_collected_context<'state>(
        &'state self,
        _context: &Self::Context<'state>,
    ) -> RunResult<bool> {
        Ok(false)
    }

    async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        self.pairs.satisfy(value, false).await
    }

    async fn report_context<'state>(
        &'state self,
        _context: &mut Self::Context<'state>,
        _source: Type<'db>,
        _union: UnionType<'db>,
        _element_count: usize,
    ) -> RunResult<()> {
        self.pairs
            .unavailable(RelationSourceOperation::TargetUnionContext)
            .await
    }
}
