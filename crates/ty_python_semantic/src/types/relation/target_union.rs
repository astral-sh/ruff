//! Target unions preserve finite alternatives, ordered element checks, and expanded-source fallback.

#[cfg(test)]
mod tests;

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::slice;

use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::{
    BoundTypeVarInstance, ErrorContext, ErrorContextTree, IntersectionType, NewType, Type,
    TypeVarBoundOrConstraints, UnionType,
};

use super::TypeRelationChecker;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTargetUnionEffects)]
    pub(in crate::types) trait TargetUnionEffects<'c, 'db: 'c> {
        type Error;
        type Elements<'state> where Self: 'state;
        type Fold<'state> where Self: 'state;
        type Context<'state> where Self: 'state;

        #[operation(child)]
        async fn finite_alternatives(&self, intersection: IntersectionType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn pair(&self, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn elements(&self, union: UnionType<'db>) -> Result<Self::Elements<'_>, Self::Error>;
        #[operation(local)]
        async fn element_count<'state>(&'state self, elements: &Self::Elements<'state>) -> Result<usize, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_element<'state>(&'state self, elements: &mut Self::Elements<'state>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn element_is_alias_like(&self, element: Type<'db>) -> Result<bool, Self::Error>;

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
        async fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn typevar_bound_or_constraints(&self, typevar: BoundTypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(child)]
        async fn source_typevar_bounds(&self, bounds: TypeVarBoundOrConstraints<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn expand_intersection(&self, intersection: IntersectionType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn newtype_concrete_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error>;

        #[operation(local)]
        async fn context_start(&self) -> Result<Self::Context<'_>, Self::Error>;
        #[operation(child)]
        async fn capture_context<'state>(&'state self, context: &mut Self::Context<'state>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn has_collected_context<'state>(&'state self, context: &Self::Context<'state>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_context<'state>(&'state self, context: &mut Self::Context<'state>, source: Type<'db>, union: UnionType<'db>, element_count: usize) -> Result<(), Self::Error>;
    }

    #[synchronous(union_has_aliases_sync)]
    #[capabilities(effects = TargetUnionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn union_has_aliases_with<'c, 'db: 'c, E: TargetUnionEffects<'c, 'db>>(
        union: UnionType<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let mut elements = effects.elements(union).await?;
        #[cursor_loop]
        while let Some(element) = effects.next_element(&mut elements).await? {
            if effects.element_is_alias_like(element).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[synchronous(check_target_union_sync)]
    #[capabilities(effects = TargetUnionEffects)]
    #[passive_values(Type::Union)]
    pub(in crate::types) async fn check_target_union_with<'c, 'db: 'c, E: TargetUnionEffects<'c, 'db>>(
        source: Type<'db>,
        union: UnionType<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let target = Type::Union(union);
        if let Type::Intersection(intersection) = source
            && let Some(alternatives) = effects.finite_alternatives(intersection).await?
        {
            return effects.pair(alternatives, target).await;
        }

        let mut context = effects.context_start().await?;
        let mut elements = effects.elements(union).await?;
        let element_count = effects.element_count(&elements).await?;
        let elements_result = {
            let mut fold = effects.fold_start().await?;
            #[passive_state]
            let mut saturated = None;
            #[cursor_loop]
            while let Some(element) = effects.next_element(&mut elements).await? {
                let next = effects.pair(source, element).await?;
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
        let result = if effects.is_trivially_always_satisfied(elements_result).await? {
            elements_result
        } else {
            // Normally non-unions cannot directly contain unions in our model due to the fact that
            // we enforce a DNF structure on our set-theoretic types. However, it *is* possible for
            // there to be a newtype of a union, for an intersection to contain a newtype of a
            // union, or for a non-inferable typevar (possibly inside an intersection) to widen to a
            // bound or set of constraints that exposes a union; this requires special handling.
            let expanded_result = match source {
                Type::TypeVar(typevar) => {
                    if !effects.typevar_is_inferable(typevar).await?
                        && let Some(bounds) = effects.typevar_bound_or_constraints(typevar).await?
                    {
                        effects.source_typevar_bounds(bounds, target).await?
                    } else {
                        effects.never().await?
                    }
                }
                Type::Intersection(intersection) => {
                    if effects.should_expand(intersection).await? {
                        let expanded = effects.expand_intersection(intersection).await?;
                        effects.pair(expanded, target).await?
                    } else {
                        effects.never().await?
                    }
                }
                Type::NewTypeInstance(newtype) => {
                    let concrete_base = effects.newtype_concrete_base(newtype).await?;
                    if matches!(concrete_base, Type::Union(_)) {
                        effects.pair(concrete_base, target).await?
                    } else {
                        effects.never().await?
                    }
                }
                _ => effects.never().await?,
            };
            effects.disjoin(elements_result, expanded_result).await?
        };

        if effects.has_collected_context(&context).await?
            && effects.is_never_satisfied(result).await?
        {
            effects.report_context(&mut context, source, union, element_count).await?;
        }
        Ok(result)
    }
}

pub(in crate::types) struct TargetUnionContext<'state, 'db> {
    elements: Vec<ErrorContextTree<'db>>,
    tree: Option<&'state ErrorContextTree<'db>>,
}

pub(super) struct InlineTargetUnionEffects<'db, 'check, 'a, 'c> {
    db: &'db dyn Db,
    checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'db, 'check, 'a, 'c> InlineTargetUnionEffects<'db, 'check, 'a, 'c> {
    pub(super) fn new(db: &'db dyn Db, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self { db, checker }
    }
}

impl<'c, 'db: 'c> SynchronousTargetUnionEffects<'c, 'db>
    for InlineTargetUnionEffects<'db, '_, '_, 'c>
{
    type Error = Infallible;
    type Elements<'state>
        = slice::Iter<'db, Type<'db>>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;
    type Context<'state>
        = TargetUnionContext<'state, 'db>
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

    fn elements(&self, union: UnionType<'db>) -> Result<Self::Elements<'_>, Infallible> {
        Ok(union.elements(self.db).iter())
    }

    fn element_count<'state>(
        &'state self,
        elements: &Self::Elements<'state>,
    ) -> Result<usize, Infallible> {
        Ok(elements.len())
    }

    fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(elements.next().copied())
    }

    fn fold_start(&self) -> Result<Self::Fold<'_>, Infallible> {
        Ok(ConstraintFold::new(
            self.checker.constraints,
            ConstraintFoldKind::Any,
        ))
    }

    fn element_is_alias_like(&self, element: Type<'db>) -> Result<bool, Infallible> {
        Ok(element.is_alias_like())
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

    fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(typevar.is_inferable(self.db, self.checker.inferable))
    }

    fn typevar_bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Infallible> {
        Ok(typevar
            .typevar(self.db)
            .bound_or_constraints(self.db, self.checker.env))
    }

    fn source_typevar_bounds(
        &self,
        bounds: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self
            .checker
            .check_source_typevar_bounds(self.db, bounds, target))
    }

    fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Infallible> {
        Ok(self
            .checker
            .should_expand_intersection(self.db, intersection))
    }

    fn expand_intersection(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(intersection.with_expanded_typevars_and_newtypes(self.db, self.checker.env))
    }

    fn newtype_concrete_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(newtype.concrete_base_type(self.db))
    }

    fn context_start(&self) -> Result<Self::Context<'_>, Infallible> {
        Ok(TargetUnionContext {
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
        source: Type<'db>,
        union: UnionType<'db>,
        element_count: usize,
    ) -> Result<(), Infallible> {
        let elements_without_context = element_count - context.elements.len();
        if elements_without_context > 0 && elements_without_context < element_count {
            context.elements.push(ErrorContextTree::from_context(
                ErrorContext::NotAssignableToNOtherUnionElements {
                    n: elements_without_context,
                },
                self.checker.relation,
            ));
        }
        self.checker.set_context(
            ErrorContext::NotAssignableToAnyUnionElement {
                source,
                union: Type::Union(union),
            },
            std::mem::take(&mut context.elements),
        );
        Ok(())
    }
}
