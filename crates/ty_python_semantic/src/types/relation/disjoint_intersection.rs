//! Intersection disjointness tries exact finite alternatives before structural comparisons.

#[cfg(test)]
mod tests;

use std::convert::Infallible;
use std::ops::ControlFlow;

use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::{IntersectionType, Type};

use super::{DisjointnessChecker, TypeRelation};

#[derive(Clone, Copy)]
pub(in crate::types) enum DisjointIntersectionOperands<'db> {
    Both {
        left: IntersectionType<'db>,
        right: IntersectionType<'db>,
    },
    Left {
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    },
    Right {
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    },
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDisjointIntersectionEffects)]
    pub(in crate::types) trait DisjointIntersectionEffects<'c, 'db: 'c> {
        type Error;
        type Elements<'state> where Self: 'state;
        type Fold<'state> where Self: 'state;

        #[operation(child)]
        async fn finite_alternatives(&self, intersection: IntersectionType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn disjoint_pair(&self, left: Type<'db>, right: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn guarded_structural(&self, left: Type<'db>, right: Type<'db>, operands: DisjointIntersectionOperands<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

        #[operation(child)]
        async fn positive_elements(&self, intersection: IntersectionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(child)]
        async fn negative_elements(&self, intersection: IntersectionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_element<'state>(&'state self, elements: &mut Self::Elements<'state>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn subtyping_pair(&self, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

        #[operation(local)]
        async fn fold_start(&self) -> Result<Self::Fold<'_>, Self::Error>;
        #[operation(child)]
        async fn fold_push<'state>(&'state self, fold: &mut Self::Fold<'state>, next: ConstraintSet<'db, 'c>) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;
        #[operation(child)]
        async fn fold_finish<'state>(&'state self, fold: &mut Self::Fold<'state>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn is_trivially_always_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn disjoin(&self, left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[synchronous(check_disjoint_intersection_sync)]
    #[capabilities(effects = DisjointIntersectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_disjoint_intersection_with<'c, 'db: 'c, E: DisjointIntersectionEffects<'c, 'db>>(
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        match operands {
            DisjointIntersectionOperands::Both {
                left: left_intersection,
                right: right_intersection,
            } => {
                if let Some(alternatives) = effects.finite_alternatives(left_intersection).await? {
                    return effects.disjoint_pair(alternatives, right).await;
                }
                if let Some(alternatives) = effects.finite_alternatives(right_intersection).await? {
                    return effects.disjoint_pair(left, alternatives).await;
                }
            }
            DisjointIntersectionOperands::Left { intersection, other } => {
                if let Some(alternatives) = effects.finite_alternatives(intersection).await? {
                    return effects.disjoint_pair(alternatives, other).await;
                }
            }
            DisjointIntersectionOperands::Right { intersection, other } => {
                if let Some(alternatives) = effects.finite_alternatives(intersection).await? {
                    return effects.disjoint_pair(other, alternatives).await;
                }
            }
        }
        effects.guarded_structural(left, right, operands).await
    }

    /// Fall back to structural disjointness for intersections without an exact finite expansion.
    ///
    /// An intersection is disjoint from another type if any positive component is disjoint from
    /// that type, or if the other type is covered by one of the intersection's negative elements.
    #[synchronous(check_disjoint_intersection_structural_sync)]
    #[capabilities(effects = DisjointIntersectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_disjoint_intersection_structural_with<'c, 'db: 'c, E: DisjointIntersectionEffects<'c, 'db>>(
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let (intersection, other) = match operands {
            DisjointIntersectionOperands::Both { left, .. } => (left, right),
            DisjointIntersectionOperands::Left { intersection, other }
            | DisjointIntersectionOperands::Right { intersection, other } => (intersection, other),
        };
        let positive_result = {
            let mut elements = effects.positive_elements(intersection).await?;
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(positive) = effects.next_element(&mut elements).await? {
                let next = effects.disjoint_pair(positive, other).await?;
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
        if effects.is_trivially_always_satisfied(positive_result).await? {
            return Ok(positive_result);
        }

        let other_result = {
            let mut elements = match operands {
                DisjointIntersectionOperands::Both { right, .. } => effects.positive_elements(right).await?,
                DisjointIntersectionOperands::Left { intersection, .. }
                | DisjointIntersectionOperands::Right { intersection, .. } => effects.negative_elements(intersection).await?,
            };
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(element) = effects.next_element(&mut elements).await? {
                let next = match operands {
                    DisjointIntersectionOperands::Both { .. } => effects.disjoint_pair(element, left).await?,
                    // A & B & Not[C] is disjoint from C
                    DisjointIntersectionOperands::Left { other, .. }
                    | DisjointIntersectionOperands::Right { other, .. } => effects.subtyping_pair(other, element).await?,
                };
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
        effects.disjoin(positive_result, other_result).await
    }
}

pub(super) struct InlineDisjointIntersectionEffects<'db, 'check, 'a, 'c> {
    db: &'db dyn Db,
    checker: &'check DisjointnessChecker<'a, 'c, 'db>,
}

impl<'db, 'check, 'a, 'c> InlineDisjointIntersectionEffects<'db, 'check, 'a, 'c> {
    pub(super) fn new(db: &'db dyn Db, checker: &'check DisjointnessChecker<'a, 'c, 'db>) -> Self {
        Self { db, checker }
    }
}

impl<'c, 'db: 'c> SynchronousDisjointIntersectionEffects<'c, 'db>
    for InlineDisjointIntersectionEffects<'db, '_, '_, 'c>
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

    fn finite_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(intersection.finite_alternative_union(self.db, self.checker.env))
    }

    fn disjoint_pair(
        &self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_type_pair(self.db, left, right))
    }

    fn guarded_structural(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.with_recursion_guard(self.db, left, right, || {
            check_disjoint_intersection_structural_sync(left, right, operands, self)
                .unwrap_or_else(|never| match never {})
        }))
    }

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

    fn subtyping_pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self
            .checker
            .as_relation_checker(TypeRelation::Subtyping)
            .check_type_pair(self.db, source, target))
    }

    fn fold_start(&self) -> Result<Self::Fold<'_>, Infallible> {
        Ok(ConstraintFold::new(
            self.checker.constraints,
            ConstraintFoldKind::Any,
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

    fn is_trivially_always_satisfied(
        &self,
        value: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Infallible> {
        value.verify_builder(self.checker.constraints);
        Ok(value.is_trivially_always_satisfied())
    }

    fn disjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(left.or(self.db, self.checker.constraints, || right))
    }
}
