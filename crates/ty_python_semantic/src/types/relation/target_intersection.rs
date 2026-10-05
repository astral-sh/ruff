//! Target intersections compare positive and negative elements in separate ordered folds.

#[cfg(test)]
mod tests;

use std::convert::Infallible;
use std::ops::ControlFlow;

use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::{ErrorContext, IntersectionType, Type};

use super::{TypeRelation, TypeRelationChecker};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTargetIntersectionEffects)]
    pub(in crate::types) trait TargetIntersectionEffects<'c, 'db: 'c> {
        type Error;
        type Elements<'state> where Self: 'state;
        type Fold<'state> where Self: 'state;

        #[operation(child)]
        async fn positive_elements(&self, intersection: IntersectionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(child)]
        async fn negative_elements(&self, intersection: IntersectionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_element<'state>(&'state self, elements: &mut Self::Elements<'state>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn positive_pair(&self, source: Type<'db>, positive: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn has_context(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_context(&self, source: Type<'db>, positive: Type<'db>, intersection: IntersectionType<'db>) -> Result<(), Self::Error>;

        #[operation(local)]
        async fn fold_start(&self) -> Result<Self::Fold<'_>, Self::Error>;
        #[operation(child)]
        async fn fold_push<'state>(&'state self, fold: &mut Self::Fold<'state>, next: ConstraintSet<'db, 'c>) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;
        #[operation(child)]
        async fn fold_finish<'state>(&'state self, fold: &mut Self::Fold<'state>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn is_trivially_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn bottom_materialization(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn disjoint_pair(&self, source: Type<'db>, negative: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn conjoin(&self, left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[synchronous(check_target_intersection_sync)]
    #[capabilities(effects = TargetIntersectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_target_intersection_with<'c, 'db: 'c, E: TargetIntersectionEffects<'c, 'db>>(
        source: Type<'db>,
        intersection: IntersectionType<'db>,
        relation: TypeRelation,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let positive_result = {
            let mut elements = effects.positive_elements(intersection).await?;
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(positive) = effects.next_element(&mut elements).await? {
                let constraint_set = effects.positive_pair(source, positive).await?;
                if effects.has_context().await?
                    && effects.is_never_satisfied(constraint_set).await?
                {
                    effects.report_context(source, positive, intersection).await?;
                }
                if let ControlFlow::Break(result) = effects.fold_push(&mut fold, constraint_set).await? {
                    saturated = Some(result);
                    break;
                }
            }
            match saturated {
                Some(result) => result,
                None => effects.fold_finish(&mut fold).await?,
            }
        };
        if effects.is_trivially_never_satisfied(positive_result).await? {
            return Ok(positive_result);
        }

        // For subtyping, we would want to check whether the *top materialization* of
        // `source` is disjoint from the *top materialization* of `negative`. As an
        // optimization, however, we can avoid this explicit transformation here, since
        // our `Type::is_disjoint_from` implementation already only returns true for
        // `T.is_disjoint_from(U)` if the *top materialization* of `T` is disjoint from the
        // *top materialization* of `U`.
        //
        // Note that the implementation of redundancy here may be too strict from a
        // theoretical perspective: under redundancy, `T <: ~U` if `Bottom[T]` is disjoint
        // from `Top[U]` and `Bottom[U]` is disjoint from `Top[T]`. It's possible that this
        // could be improved. For now, however, we err on the side of strictness for our
        // redundancy implementation: a fully complete implementation of redundancy may
        // lead to non-transitivity (highly undesirable); and pragmatically, a full
        // implementation of redundancy may not generally lead to simpler types in many
        // situations.
        let source_ty = match relation {
            TypeRelation::Subtyping
            | TypeRelation::Redundancy { .. }
            | TypeRelation::SubtypingAssuming => source,
            TypeRelation::Assignability => effects.bottom_materialization(source).await?,
        };
        let negative_result = {
            let mut elements = effects.negative_elements(intersection).await?;
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(negative) = effects.next_element(&mut elements).await? {
                let negative = match relation {
                    TypeRelation::Subtyping
                    | TypeRelation::Redundancy { .. }
                    | TypeRelation::SubtypingAssuming => negative,
                    TypeRelation::Assignability => effects.bottom_materialization(negative).await?,
                };
                let next = effects.disjoint_pair(source_ty, negative).await?;
                if let ControlFlow::Break(result) = effects.fold_push(&mut fold, next).await? {
                    saturated = Some(result);
                    break;
                }
            }
            match saturated {
                Some(result) => result,
                None => effects.fold_finish(&mut fold).await?,
            }
        };
        effects.conjoin(positive_result, negative_result).await
    }
}

pub(super) struct InlineTargetIntersectionEffects<'db, 'check, 'a, 'c> {
    db: &'db dyn Db,
    checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'db, 'check, 'a, 'c> InlineTargetIntersectionEffects<'db, 'check, 'a, 'c> {
    pub(super) fn new(db: &'db dyn Db, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self { db, checker }
    }
}

impl<'c, 'db: 'c> SynchronousTargetIntersectionEffects<'c, 'db>
    for InlineTargetIntersectionEffects<'db, '_, '_, 'c>
{
    type Error = Infallible;
    type Elements<'state>
        = Elements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;

    fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Self::Elements<'_>, Infallible> {
        Ok(Elements::Positive(intersection.positive(self.db).iter()))
    }

    fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Self::Elements<'_>, Infallible> {
        Ok(Elements::Negative(intersection.negative(self.db).iter()))
    }

    fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(match elements {
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
    }

    fn positive_pair(
        &self,
        source: Type<'db>,
        positive: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_type_pair(self.db, source, positive))
    }

    fn has_context(&self) -> Result<bool, Infallible> {
        Ok(self.checker.report_context().is_some())
    }

    fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Infallible> {
        Ok(value.is_never_satisfied(self.db, self.checker.env))
    }

    fn report_context(
        &self,
        source: Type<'db>,
        positive: Type<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<(), Infallible> {
        if let Some(context) = self.checker.report_context() {
            context.push(ErrorContext::NotAssignableToIntersectionElement {
                source,
                element: positive,
                intersection: Type::Intersection(intersection),
            });
        }
        Ok(())
    }

    fn fold_start(&self) -> Result<Self::Fold<'_>, Infallible> {
        Ok(ConstraintFold::new(
            self.checker.constraints,
            ConstraintFoldKind::All,
        ))
    }

    fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Infallible> {
        Ok(fold.push(next))
    }

    fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(fold.finish_borrowed())
    }

    fn is_trivially_never_satisfied(
        &self,
        value: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Infallible> {
        value.verify_builder(self.checker.constraints);
        Ok(value.is_trivially_never_satisfied())
    }

    fn bottom_materialization(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.bottom_materialization(self.db, self.checker.env))
    }

    fn disjoint_pair(
        &self,
        source: Type<'db>,
        negative: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self
            .checker
            .as_disjointness_checker()
            .check_type_pair(self.db, source, negative))
    }

    fn conjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(left.and(self.db, self.checker.constraints, || right))
    }
}
