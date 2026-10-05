//! Context targets and tuple annotations preserve the ordinary lookup and filtering order.

use std::borrow::Cow;
use std::convert::Infallible;

use crate::types::constraints::ConstraintSetBuilder;
use crate::types::{
    DiscardDisjointUnionElementsResult, GenericContext, KnownClass, Specialization,
    StaticClassLiteral, Type, TypeVarSet, UnionType,
};
use crate::{Db, ProgramEnvironment};

/// Ordinary semantic operations used while selecting contextual tuple annotations.
pub(in crate::types) struct OrdinaryTypeContextEffects<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

pub(in crate::types) struct TypeContextFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeContextEffects)]
    pub(in crate::types) trait TypeContextEffects<'db> {
        type Error;
        type Constraints;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn union_like(&self, ty: Type<'db>) -> Result<Option<UnionType<'db>>, Self::Error>;
        #[operation(child)]
        async fn has_aliases(&self, union: UnionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn expand_aliases(&self, union: UnionType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_targets(&self, union: UnionType<'db>) -> Result<Cow<'db, [Type<'db>]>, Self::Error>;
        #[operation(local)]
        async fn singleton_target(&self, ty: Type<'db>) -> Result<Cow<'db, [Type<'db>]>, Self::Error>;
        #[operation(child)]
        async fn known_class(&self, class: KnownClass) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn generic_context(&self, class: StaticClassLiteral<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn inferable_typevars(&self, context: GenericContext<'db>) -> Result<TypeVarSet<'db>, Self::Error>;
        #[operation(child)]
        async fn homogeneous_unknown_tuple(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_constraints(&self) -> Result<Self::Constraints, Self::Error>;
        #[operation(child)]
        async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn union_for_filter(&self, ty: Type<'db>) -> Result<Option<UnionType<'db>>, Self::Error>;
        #[operation(child)]
        async fn filter_disjoint(&self, union: UnionType<'db>, target: Type<'db>, constraints: &Self::Constraints, inferable: TypeVarSet<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn discard_disjoint(&self, annotation: Type<'db>, target: Type<'db>, inferable: TypeVarSet<'db>) -> Result<DiscardDisjointUnionElementsResult<'db>, Self::Error>;
        #[operation(child)]
        async fn class_specialization(&self, annotation: Type<'db>) -> Result<Option<(StaticClassLiteral<'db>, Specialization<'db>)>, Self::Error>;
        #[operation(child)]
        async fn specialization_of<'expected>(&self, annotation: Type<'db>, expected: StaticClassLiteral<'expected>) -> Result<Option<Specialization<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl TypeContextFacts {
        fn same_class<'left, 'right>(&self, left: StaticClassLiteral<'left>, right: StaticClassLiteral<'right>) -> bool {
            left == right
        }

        fn all_disjoint<'db>(&self, original: Type<'db>, filtered: Type<'db>) -> bool {
            filtered.is_never() && !original.is_never()
        }

        fn retained_or_never<'db>(&self, result: DiscardDisjointUnionElementsResult<'db>) -> Type<'db> {
            result.or_never()
        }
    }

    /// Returns union members that may be tried as separate contextual inference targets.
    #[synchronous(narrow_targets_sync)]
    #[capabilities(effects = TypeContextEffects)]
    #[passive_values()]
    pub(in crate::types) async fn narrow_targets_with<'db, E: TypeContextEffects<'db>>(
        annotation: Option<Type<'db>>,
        effects: &E,
    ) -> Result<Option<Cow<'db, [Type<'db>]>>, E::Error> {
        effects.checkpoint().await?;
        let Some(annotation) = annotation else { return Ok(None); };
        let Some(union) = effects.union_like(annotation).await? else { return Ok(None); };
        let targets = if effects.has_aliases(union).await? {
            let expanded = effects.expand_aliases(union).await?;
            if let Some(union) = effects.union_like(expanded).await? {
                effects.union_targets(union).await?
            } else {
                effects.singleton_target(expanded).await?
            }
        } else {
            effects.union_targets(union).await?
        };

        // TODO: We could theoretically attempt to narrow to every element of
        // the power set of this union. However, this leads to an exponential
        // explosion of inference attempts, and is rarely needed in practice.
        Ok(Some(targets))
    }

    /// Returns the resolved union, or `None` so the caller can preserve a nonunion annotation.
    #[synchronous(union_for_filter_sync)]
    #[capabilities(effects = TypeContextEffects)]
    #[passive_values()]
    pub(in crate::types) async fn union_for_filter_with<'db, E: TypeContextEffects<'db>>(
        annotation: Type<'db>,
        effects: &E,
    ) -> Result<Option<UnionType<'db>>, E::Error> {
        effects.checkpoint().await?;
        match effects.resolve_alias(annotation).await? {
            Type::Union(union) => Ok(Some(union)),
            _ => Ok(None),
        }
    }

    /// Removes disjoint union members, preserving nonunion inputs and the all-disjoint result.
    #[synchronous(discard_disjoint_sync)]
    #[capabilities(effects = TypeContextEffects, facts = TypeContextFacts)]
    #[passive_values(DiscardDisjointUnionElementsResult::AllDisjoint, DiscardDisjointUnionElementsResult::Retained)]
    pub(in crate::types) async fn discard_disjoint_with<'db, E: TypeContextEffects<'db>>(
        annotation: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
        facts: TypeContextFacts,
        effects: &E,
    ) -> Result<DiscardDisjointUnionElementsResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let constraints = effects.new_constraints().await?;
        let filtered = match effects.union_for_filter(annotation).await? {
            Some(union) => effects.filter_disjoint(union, target, &constraints, inferable).await?,
            None => annotation,
        };
        if facts.all_disjoint(annotation, filtered) {
            Ok(DiscardDisjointUnionElementsResult::AllDisjoint)
        } else {
            Ok(DiscardDisjointUnionElementsResult::Retained(filtered))
        }
    }

    /// Removes annotation union members disjoint from a homogeneous unknown tuple target.
    /// The builtin tuple class's inferable variables are constructed before filtering begins.
    #[synchronous(filter_tuple_annotation_sync)]
    #[capabilities(effects = TypeContextEffects, facts = TypeContextFacts)]
    #[passive_values(KnownClass::Tuple, TypeVarSet::None)]
    pub(in crate::types) async fn filter_tuple_annotation_with<'db, E: TypeContextEffects<'db>>(
        annotation: Type<'db>,
        facts: TypeContextFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        let inferable = match effects.known_class(KnownClass::Tuple).await? {
            Some(class) => match effects.generic_context(class).await? {
                Some(context) => effects.inferable_typevars(context).await?,
                None => TypeVarSet::None,
            },
            None => TypeVarSet::None,
        };
        let target = effects.homogeneous_unknown_tuple().await?;
        let filtered = effects.discard_disjoint(annotation, target, inferable).await?;
        Ok(facts.retained_or_never(filtered))
    }

    /// Returns a specialization only when the annotation has exactly the requested known class.
    #[synchronous(known_specialization_sync)]
    #[capabilities(effects = TypeContextEffects)]
    #[passive_values()]
    pub(in crate::types) async fn known_specialization_with<'db, E: TypeContextEffects<'db>>(
        annotation: Type<'db>,
        known_class: KnownClass,
        effects: &E,
    ) -> Result<Option<Specialization<'db>>, E::Error> {
        effects.checkpoint().await?;
        let Some(expected) = effects.known_class(known_class).await? else { return Ok(None); };
        effects.specialization_of(annotation, expected).await
    }

    /// Returns a specialization only when its class identity equals the requested class.
    #[synchronous(specialization_of_sync)]
    #[capabilities(effects = TypeContextEffects, facts = TypeContextFacts)]
    #[passive_values()]
    pub(in crate::types) async fn specialization_of_with<'db, 'expected, E: TypeContextEffects<'db>>(
        annotation: Type<'db>,
        expected: StaticClassLiteral<'expected>,
        facts: TypeContextFacts,
        effects: &E,
    ) -> Result<Option<Specialization<'db>>, E::Error> {
        effects.checkpoint().await?;
        match effects.class_specialization(annotation).await? {
            Some((class, specialization)) if facts.same_class(class, expected) => Ok(Some(specialization)),
            _ => Ok(None),
        }
    }
}

/// Removes annotation union members disjoint from a homogeneous unknown tuple target,
/// with the same eager builtin-tuple setup used by controlled inference.
pub(in crate::types) fn filter_tuple_annotation<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    annotation: Type<'db>,
) -> Type<'db> {
    match filter_tuple_annotation_sync(
        annotation,
        TypeContextFacts,
        &OrdinaryTypeContextEffects { db, env },
    ) {
        Ok(annotation) => annotation,
        Err(never) => match never {},
    }
}

impl<'db> SynchronousTypeContextEffects<'db> for OrdinaryTypeContextEffects<'_, 'db> {
    type Error = Infallible;
    type Constraints = ConstraintSetBuilder<'db>;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn union_like(&self, ty: Type<'db>) -> Result<Option<UnionType<'db>>, Infallible> {
        Ok(ty.as_union_like(self.db))
    }

    fn has_aliases(&self, union: UnionType<'db>) -> Result<bool, Infallible> {
        Ok(union.has_aliases(self.db))
    }

    fn expand_aliases(&self, union: UnionType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(union.expand_aliases(self.db, self.env))
    }

    fn union_targets(&self, union: UnionType<'db>) -> Result<Cow<'db, [Type<'db>]>, Infallible> {
        Ok(Cow::Borrowed(union.elements(self.db)))
    }

    fn singleton_target(&self, ty: Type<'db>) -> Result<Cow<'db, [Type<'db>]>, Infallible> {
        Ok(Cow::Owned(vec![ty]))
    }

    fn known_class(
        &self,
        class: KnownClass,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(class.try_to_class_literal(self.db, self.env))
    }

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(class.generic_context(self.db))
    }

    fn inferable_typevars(
        &self,
        context: GenericContext<'db>,
    ) -> Result<TypeVarSet<'db>, Infallible> {
        Ok(context.inferable_typevars(self.db))
    }

    fn homogeneous_unknown_tuple(&self) -> Result<Type<'db>, Infallible> {
        Ok(Type::homogeneous_tuple(self.db, self.env, Type::unknown()))
    }

    fn new_constraints(&self) -> Result<Self::Constraints, Infallible> {
        Ok(ConstraintSetBuilder::new())
    }

    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.resolve_type_alias(self.db))
    }

    fn union_for_filter(&self, ty: Type<'db>) -> Result<Option<UnionType<'db>>, Infallible> {
        union_for_filter_sync(ty, self)
    }

    fn filter_disjoint(
        &self,
        union: UnionType<'db>,
        target: Type<'db>,
        constraints: &Self::Constraints,
        inferable: TypeVarSet<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(union.filter_expanding_aliases(self.db, self.env, |elem| {
            !elem
                .when_disjoint_from(self.db, self.env, target, constraints, inferable)
                .is_always_satisfied(self.db, self.env)
        }))
    }

    fn discard_disjoint(
        &self,
        annotation: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Result<DiscardDisjointUnionElementsResult<'db>, Infallible> {
        discard_disjoint_sync(annotation, target, inferable, TypeContextFacts, self)
    }

    fn class_specialization(
        &self,
        annotation: Type<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Specialization<'db>)>, Infallible> {
        Ok(annotation.class_specialization(self.db, self.env))
    }

    fn specialization_of<'expected>(
        &self,
        annotation: Type<'db>,
        expected: StaticClassLiteral<'expected>,
    ) -> Result<Option<Specialization<'db>>, Infallible> {
        specialization_of_sync(annotation, expected, TypeContextFacts, self)
    }
}
