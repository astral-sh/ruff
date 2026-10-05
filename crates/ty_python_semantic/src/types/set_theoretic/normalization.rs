use std::convert::Infallible;
use std::slice;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::ProgramEnvironment;
use crate::types::normalization::{
    OrdinaryNormalizationEffects, RecursiveNormalizationFacts, RecursiveNormalizationRequest,
    recursive_normalize_sync,
};
use crate::types::{RecursivelyDefined, Type, UnionBuilder, UnionType};

pub(in crate::types) struct UnionNormalizationFacts;

shared_semantic_family! {
    #[synchronous(SynchronousUnionNormalizationEffects)]
    pub(in crate::types) trait UnionNormalizationEffects<'db> {
        type Error;

        #[operation(local)]
        async fn new_recovery_union(&self, env: &ProgramEnvironment<'db>) -> Result<UnionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn union_recursion(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Self::Error>;
        #[operation(local)]
        async fn merge_recursion(&self, builder: &mut UnionBuilder<'db>, recursion: RecursivelyDefined) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &mut slice::Iter<'_, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn normalize_child(&self, request: RecursiveNormalizationRequest<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl UnionNormalizationFacts {
        fn elements<'a, 'db>(&self, types: &'a [Type<'db>]) -> slice::Iter<'a, Type<'db>> {
            types.iter()
        }

        fn child<'db>(&self, ty: Type<'db>, divergent: Type<'db>, nested: bool) -> RecursiveNormalizationRequest<'db> {
            RecursiveNormalizationRequest { ty, divergent, nested }
        }

        fn same_marker<'db>(&self, ty: Type<'db>, divergent: Type<'db>) -> bool {
            ty.same_divergent_marker(divergent)
        }

        fn normalized_or<'db>(&self, normalized: Option<Type<'db>>, divergent: Type<'db>) -> Type<'db> {
            normalized.unwrap_or(divergent)
        }
    }

    #[synchronous(union_normalize_sync)]
    #[capabilities(effects = UnionNormalizationEffects, facts = UnionNormalizationFacts)]
    #[passive_values(RecursivelyDefined::Yes)]
    pub(in crate::types) async fn union_normalize_with<'db, E: UnionNormalizationEffects<'db>>(
        union: UnionType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
        effects: &E,
        facts: UnionNormalizationFacts,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let mut builder = effects.new_recovery_union(env).await?;
        let recursion = effects.union_recursion(union).await?;
        effects.merge_recursion(&mut builder, recursion).await?;
        #[passive_state]
        let mut empty = true;
        let types = effects.union_elements(union).await?;
        let mut elements = facts.elements(types);
        #[cursor_loop]
        while let Some(ty) = effects.next_element(&mut elements).await? {
            if nested {
                // list[T | Divergent] => list[Divergent]
                let Some(ty) = effects.normalize_child(facts.child(ty, divergent, nested), env).await? else {
                    return Ok(None);
                };
                if facts.same_marker(ty, divergent) {
                    return Ok(Some(ty));
                }
                effects.union_add(&mut builder, ty).await?;
                empty = false;
            } else {
                // `Divergent` in a union type does not mean true divergence, so we skip it if not nested.
                // e.g. T | Divergent == T | (T | (T | (T | ...))) == T
                if facts.same_marker(ty, divergent) {
                    effects.merge_recursion(&mut builder, RecursivelyDefined::Yes).await?;
                    continue;
                }
                let normalized = effects.normalize_child(facts.child(ty, divergent, nested), env).await?;
                effects.union_add(&mut builder, facts.normalized_or(normalized, divergent)).await?;
                empty = false;
            }
        }
        if empty {
            effects.union_add(&mut builder, divergent).await?;
        }
        Ok(Some(effects.union_build(builder).await?))
    }
}

impl<'db> SynchronousUnionNormalizationEffects<'db> for OrdinaryNormalizationEffects<'db> {
    type Error = Infallible;

    fn new_recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<UnionBuilder<'db>, Infallible> {
        Ok(UnionBuilder::new(self.db, env)
            .unpack_aliases(false)
            .cycle_recovery(true))
    }

    fn union_recursion(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Infallible> {
        Ok(union.recursively_defined(self.db))
    }

    fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> Result<(), Infallible> {
        builder.merge_recursively_defined(recursion);
        Ok(())
    }

    fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(union.elements(self.db))
    }

    fn next_element(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(elements.next().copied())
    }

    fn normalize_child(
        &self,
        request: RecursiveNormalizationRequest<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        recursive_normalize_sync(request, env, self, RecursiveNormalizationFacts)
    }

    fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Infallible> {
        builder.add_in_place(ty);
        Ok(())
    }

    fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Infallible> {
        Ok(builder.build())
    }
}
