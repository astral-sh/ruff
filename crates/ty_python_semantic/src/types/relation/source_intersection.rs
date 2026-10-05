//! Source intersections compare positive elements before considering expansion.

#[cfg(test)]
mod tests;

use std::convert::Infallible;
use std::ops::ControlFlow;

use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::{
    BoundTypeVarInstance, ErrorContext, ErrorContextTree, IntersectionType, NewType, Type,
};

use super::TypeRelationChecker;

pub(in crate::types) struct SourceIntersectionElements<'db> {
    pub(in crate::types) elements: Elements<'db>,
    implicit_object: bool,
}

impl<'db> SourceIntersectionElements<'db> {
    pub(in crate::types) fn new(elements: Elements<'db>, implicit_object: bool) -> Self {
        Self {
            elements,
            implicit_object,
        }
    }

    pub(in crate::types) fn after_next(&mut self, next: Option<Type<'db>>) -> Option<Type<'db>> {
        match (std::mem::take(&mut self.implicit_object), next) {
            (true, None) => Some(Type::object()),
            (_, next) => next,
        }
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSourceIntersectionEffects)]
    pub(in crate::types) trait SourceIntersectionEffects<'c, 'db: 'c> {
        type Error;
        type Elements<'state> where Self: 'state;
        type Fold<'state> where Self: 'state;
        type Context<'state> where Self: 'state;

        #[operation(child)]
        async fn finite_alternatives(&self, intersection: IntersectionType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn pair(&self, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn positive_elements(&self, intersection: IntersectionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(child)]
        async fn positive_elements_or_object(&self, intersection: IntersectionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_element<'state>(&'state self, elements: &mut Self::Elements<'state>) -> Result<Option<Type<'db>>, Self::Error>;

        #[operation(local)]
        async fn fold_start(&self) -> Result<Self::Fold<'_>, Self::Error>;
        #[operation(child)]
        async fn fold_push<'state>(&'state self, fold: &mut Self::Fold<'state>, next: ConstraintSet<'db, 'c>) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;
        #[operation(child)]
        async fn fold_finish<'state>(&'state self, fold: &mut Self::Fold<'state>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn is_trivially_always_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn never(&self) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn disjoin(&self, left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

        #[operation(child)]
        async fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn newtype_concrete_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn expand_intersection(&self, intersection: IntersectionType<'db>) -> Result<Type<'db>, Self::Error>;

        #[operation(local)]
        async fn context_start(&self) -> Result<Self::Context<'_>, Self::Error>;
        #[operation(child)]
        async fn capture_context<'state>(&'state self, context: &mut Self::Context<'state>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn has_collected_context<'state>(&'state self, context: &Self::Context<'state>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_context<'state>(&'state self, context: &mut Self::Context<'state>, intersection: IntersectionType<'db>, target: Type<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_source_intersection_sync)]
    #[capabilities(effects = SourceIntersectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_source_intersection_with<'c, 'db: 'c, E: SourceIntersectionEffects<'c, 'db>>(
        intersection: IntersectionType<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        if matches!(target, Type::LiteralValue(_))
            && let Some(alternatives) = effects.finite_alternatives(intersection).await?
        {
            return effects.pair(alternatives, target).await;
        }

        // An intersection type is a subtype of another type if at least one of its positive
        // elements is a subtype of that type. If there are no positive elements, we treat `object`
        // as the implicit positive element (e.g., `~str` is semantically `object & ~str`).
        let mut context = effects.context_start().await?;
        let positive_result = {
            let mut elements = effects.positive_elements_or_object(intersection).await?;
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(element) = effects.next_element(&mut elements).await? {
                let next = effects.pair(element, target).await?;
                effects.capture_context(&mut context).await?;
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
        let result = if effects.is_trivially_always_satisfied(positive_result).await? {
            positive_result
        } else {
            let expanded_result = if effects.should_expand(intersection).await? {
                let expanded = effects.expand_intersection(intersection).await?;
                effects.pair(expanded, target).await?
            } else {
                effects.never().await?
            };
            effects.disjoin(positive_result, expanded_result).await?
        };

        if effects.has_collected_context(&context).await?
            && effects.is_never_satisfied(result).await?
        {
            effects.report_context(&mut context, intersection, target).await?;
        }
        Ok(result)
    }

    #[synchronous(should_expand_source_intersection_sync)]
    #[capabilities(effects = SourceIntersectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn should_expand_source_intersection_with<'c, 'db: 'c, E: SourceIntersectionEffects<'c, 'db>>(
        intersection: IntersectionType<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let mut elements = effects.positive_elements(intersection).await?;
        #[cursor_loop]
        while let Some(element) = effects.next_element(&mut elements).await? {
            match element {
                Type::TypeVar(typevar) => {
                    if !effects.typevar_is_inferable(typevar).await? {
                        return Ok(true);
                    }
                }
                Type::NewTypeInstance(newtype) => {
                    if matches!(effects.newtype_concrete_base(newtype).await?, Type::Union(_)) {
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }
}

pub(in crate::types) struct SourceIntersectionContext<'state, 'db> {
    elements: Vec<ErrorContextTree<'db>>,
    tree: Option<&'state ErrorContextTree<'db>>,
}

pub(super) struct InlineSourceIntersectionEffects<'db, 'check, 'a, 'c> {
    db: &'db dyn Db,
    checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'db, 'check, 'a, 'c> InlineSourceIntersectionEffects<'db, 'check, 'a, 'c> {
    pub(super) fn new(db: &'db dyn Db, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self { db, checker }
    }
}

impl<'c, 'db: 'c> SynchronousSourceIntersectionEffects<'c, 'db>
    for InlineSourceIntersectionEffects<'db, '_, '_, 'c>
{
    type Error = Infallible;
    type Elements<'state>
        = SourceIntersectionElements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;
    type Context<'state>
        = SourceIntersectionContext<'state, 'db>
    where
        Self: 'state;

    fn finite_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(intersection.finite_alternative_union(self.db, self.checker.env))
    }

    fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_type_pair(self.db, source, target))
    }

    fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Self::Elements<'_>, Infallible> {
        Ok(SourceIntersectionElements::new(
            Elements::Positive(intersection.positive(self.db).iter()),
            false,
        ))
    }

    fn positive_elements_or_object(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Self::Elements<'_>, Infallible> {
        Ok(SourceIntersectionElements::new(
            Elements::Positive(intersection.positive(self.db).iter()),
            true,
        ))
    }

    fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        let next = match &mut elements.elements {
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        };
        Ok(elements.after_next(next))
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

    fn never(&self) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.never())
    }

    fn disjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(left.or(self.db, self.checker.constraints, || right))
    }

    fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Infallible> {
        should_expand_source_intersection_sync(intersection, self)
    }

    fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(typevar.is_inferable(self.db, self.checker.inferable))
    }

    fn newtype_concrete_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(newtype.concrete_base_type(self.db))
    }

    fn expand_intersection(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(intersection.with_expanded_typevars_and_newtypes(self.db, self.checker.env))
    }

    fn context_start(&self) -> Result<Self::Context<'_>, Infallible> {
        Ok(SourceIntersectionContext {
            elements: Vec::new(),
            tree: self
                .checker
                .context_tree
                .as_ref()
                .filter(|tree| tree.is_enabled()),
        })
    }

    fn capture_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
    ) -> Result<(), Infallible> {
        if let Some(context_tree) = context.tree {
            let child_context = context_tree.take();
            if !child_context.is_empty() {
                context.elements.push(child_context);
            }
        }
        Ok(())
    }

    fn has_collected_context<'state>(
        &'state self,
        context: &Self::Context<'state>,
    ) -> Result<bool, Infallible> {
        Ok(context.tree.is_some() && !context.elements.is_empty())
    }

    fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Infallible> {
        Ok(value.is_never_satisfied(self.db, self.checker.env))
    }

    fn report_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> Result<(), Infallible> {
        self.checker.set_context(
            ErrorContext::NoIntersectionElementAssignableToTarget {
                intersection: Type::Intersection(intersection),
                target,
            },
            std::mem::take(&mut context.elements),
        );
        Ok(())
    }
}
