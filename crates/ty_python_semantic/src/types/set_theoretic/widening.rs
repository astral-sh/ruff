use std::convert::Infallible;
use std::slice;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::tuple::{TupleLength, TupleSpec, TupleType};
use crate::types::{RecursivelyDefined, Type, UnionBuilder, UnionType};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
enum AlternativeTypes<'db> {
    Single(Type<'db>),
    Union(&'db [Type<'db>]),
}

impl<'db> AlternativeTypes<'db> {
    fn get(self, index: usize) -> Option<Type<'db>> {
        match self {
            Self::Single(ty) => (index == 0).then_some(ty),
            Self::Union(types) => types.get(index).copied(),
        }
    }
}

pub(in crate::types) struct AlternativeCursor<'db> {
    current: AlternativeTypes<'db>,
    remaining: Option<AlternativeTypes<'db>>,
    index: usize,
}

impl<'db> AlternativeCursor<'db> {
    pub(in crate::types) fn next(&mut self) -> Option<Type<'db>> {
        if let Some(ty) = self.current.get(self.index) {
            self.index += 1;
            return Some(ty);
        }
        self.current = self.remaining.take()?;
        self.index = 0;
        let ty = self.current.get(self.index)?;
        self.index += 1;
        Some(ty)
    }
}

pub(in crate::types) struct TupleElementCursor<'db> {
    prefix: slice::Iter<'db, Type<'db>>,
    variable: Option<Type<'db>>,
    suffix: slice::Iter<'db, Type<'db>>,
}

impl<'db> TupleElementCursor<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db, spec: &'db TupleSpec<'db>) -> Self {
        match spec {
            TupleSpec::Fixed(tuple) => Self {
                prefix: tuple.all_elements().iter(),
                variable: None,
                suffix: [].iter(),
            },
            TupleSpec::Variable(tuple) => Self {
                prefix: tuple.prefix_elements().iter(),
                variable: Some(tuple.variable().element_type(db)),
                suffix: tuple.suffix_elements().iter(),
            },
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<Type<'db>> {
        self.prefix
            .next()
            .copied()
            .or_else(|| self.variable.take())
            .or_else(|| self.suffix.next().copied())
    }
}

pub(in crate::types) struct TupleWideningFacts;

pub(in crate::types) struct OrdinaryTupleWideningEffects<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousTupleWideningEffects)]
    pub(in crate::types) trait TupleWideningEffects<'db> {
        type Error;

        #[operation(local)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(source)]
        async fn exact_spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, cursor: &mut AlternativeCursor<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn push_length(&self, lengths: &mut Vec<TupleLength>, length: TupleLength) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_length(&self, cursor: &mut slice::Iter<'_, TupleLength>) -> Result<Option<TupleLength>, Self::Error>;
        #[operation(local)]
        async fn tuple_elements(&self, spec: &'db TupleSpec<'db>) -> Result<TupleElementCursor<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_tuple_element(&self, cursor: &mut TupleElementCursor<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_recovery_union(&self, env: &ProgramEnvironment<'db>) -> Result<UnionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn union_recursion(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Self::Error>;
        #[operation(local)]
        async fn merge_recursion(&self, builder: &mut UnionBuilder<'db>, recursion: RecursivelyDefined) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn homogeneous_tuple(&self, env: &ProgramEnvironment<'db>, element: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TupleWideningFacts {
        fn same_type<'db>(&self, previous: Type<'db>, current: Type<'db>) -> bool {
            previous == current
        }

        fn single<'db>(&self, ty: Type<'db>) -> AlternativeTypes<'db> {
            AlternativeTypes::Single(ty)
        }

        fn union<'db>(&self, types: &'db [Type<'db>]) -> AlternativeTypes<'db> {
            AlternativeTypes::Union(types)
        }

        fn cursor<'db>(&self, types: AlternativeTypes<'db>) -> AlternativeCursor<'db> {
            AlternativeCursor { current: types, remaining: None, index: 0 }
        }

        fn chain<'db>(&self, previous: AlternativeTypes<'db>, current: AlternativeTypes<'db>) -> AlternativeCursor<'db> {
            AlternativeCursor { current: previous, remaining: Some(current), index: 0 }
        }

        fn exact_tuple<'db>(&self, ty: Type<'db>) -> Option<TupleType<'db>> {
            ty.as_nominal_instance().and_then(|instance| instance.exact_tuple())
        }

        fn empty_lengths(&self) -> Vec<TupleLength> {
            Vec::new()
        }

        fn no_lengths(&self, lengths: &[TupleLength]) -> bool {
            lengths.is_empty()
        }

        fn length(&self, spec: &TupleSpec<'_>) -> TupleLength {
            spec.len()
        }

        fn length_cursor<'a>(&self, lengths: &'a [TupleLength]) -> slice::Iter<'a, TupleLength> {
            lengths.iter()
        }

        fn same_length(&self, previous: TupleLength, current: TupleLength) -> bool {
            previous == current
        }

        fn object<'db>(&self) -> Type<'db> {
            Type::object()
        }
    }

    #[synchronous(widen_growing_tuples_sync)]
    #[capabilities(effects = TupleWideningEffects, facts = TupleWideningFacts)]
    #[passive_values(RecursivelyDefined::No, RecursivelyDefined::Yes)]
    pub(in crate::types) async fn widen_growing_tuples_with<'db, E: TupleWideningEffects<'db>>(
        previous: Type<'db>,
        current: Type<'db>,
        env: &ProgramEnvironment<'db>,
        facts: TupleWideningFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        if facts.same_type(previous, current) {
            return Ok(None);
        }
        let previous_types = match previous {
            Type::Union(union) => facts.union(effects.union_elements(union).await?),
            ty => facts.single(ty),
        };
        let current_types = match current {
            Type::Union(union) => facts.union(effects.union_elements(union).await?),
            ty => facts.single(ty),
        };
        let mut previous_lengths = facts.empty_lengths();
        let mut previous_cursor = facts.cursor(previous_types);
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut previous_cursor).await? {
            if let Some(tuple) = facts.exact_tuple(ty) {
                let spec = effects.exact_spec(tuple).await?;
                effects.push_length(&mut previous_lengths, facts.length(spec)).await?;
            }
        }
        if facts.no_lengths(&previous_lengths) {
            return Ok(None);
        }
        #[passive_state]
        let mut has_new_length = false;
        let mut current_cursor = facts.cursor(current_types);
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut current_cursor).await? {
            if let Some(tuple) = facts.exact_tuple(ty) {
                let spec = effects.exact_spec(tuple).await?;
                let length = facts.length(spec);
                #[passive_state]
                let mut seen = false;
                let mut lengths = facts.length_cursor(&previous_lengths);
                #[cursor_loop]
                while let Some(previous_length) = effects.next_length(&mut lengths).await? {
                    if facts.same_length(previous_length, length) {
                        seen = true;
                        break;
                    }
                }
                if !seen {
                    has_new_length = true;
                    break;
                }
            }
        }
        if !has_new_length {
            return Ok(None);
        }

        // Recovery cannot perform relation queries, including when combining tuple elements.
        // Mark those elements recursive so growing literal unions also widen promptly.
        let mut elements = effects.new_recovery_union(env).await?;
        effects.merge_recursion(&mut elements, RecursivelyDefined::Yes).await?;
        let mut result = effects.new_recovery_union(env).await?;
        let recursion = match current {
            Type::Union(union) => effects.union_recursion(union).await?,
            _ => RecursivelyDefined::No,
        };
        effects.merge_recursion(&mut result, recursion).await?;
        // During the first cycle iterations, the caller can discard previous alternatives.
        // Retain their tuple elements for widening without restoring unrelated alternatives.
        let mut current_cursor = facts.cursor(current_types);
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut current_cursor).await? {
            match facts.exact_tuple(ty) {
                Some(tuple) => {
                    let _ = effects.exact_spec(tuple).await?;
                }
                None => effects.union_add(&mut result, ty).await?,
            }
        }
        let mut tuple_cursor = facts.chain(previous_types, current_types);
        #[cursor_loop]
        while let Some(ty) = effects.next_type(&mut tuple_cursor).await? {
            if let Some(tuple) = facts.exact_tuple(ty) {
                let spec = effects.exact_spec(tuple).await?;
                let mut cursor = effects.tuple_elements(spec).await?;
                #[cursor_loop]
                while let Some(element) = effects.next_tuple_element(&mut cursor).await? {
                    effects.union_add(&mut elements, element).await?;
                }
            }
        }
        let element_type = match effects.union_build(elements).await? {
            // `tuple[Never, ...]` normalizes to the empty tuple, which does not contain
            // fixed-length types like `tuple[Never]`. Preserve a static upper bound.
            Type::Never => facts.object(),
            element_type => element_type,
        };
        let widened = effects.homogeneous_tuple(env, element_type).await?;
        effects.union_add(&mut result, widened).await?;
        Ok(Some(effects.union_build(result).await?))
    }

    #[synchronous(recovery_union_sync)]
    #[capabilities(effects = TupleWideningEffects)]
    #[passive_values()]
    pub(in crate::types) async fn recovery_union_with<'db, E: TupleWideningEffects<'db>>(
        previous: Type<'db>,
        current: Type<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut builder = effects.new_recovery_union(env).await?;
        effects.union_add(&mut builder, previous).await?;
        effects.union_add(&mut builder, current).await?;
        effects.union_build(builder).await
    }
}

impl<'db> SynchronousTupleWideningEffects<'db> for OrdinaryTupleWideningEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(union.elements(self.db))
    }

    fn exact_spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Infallible> {
        Ok(tuple.tuple(self.db))
    }

    fn next_type(
        &self,
        cursor: &mut AlternativeCursor<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn push_length(
        &self,
        lengths: &mut Vec<TupleLength>,
        length: TupleLength,
    ) -> Result<(), Infallible> {
        lengths.push(length);
        Ok(())
    }

    fn next_length(
        &self,
        cursor: &mut slice::Iter<'_, TupleLength>,
    ) -> Result<Option<TupleLength>, Infallible> {
        Ok(cursor.next().copied())
    }

    fn tuple_elements(
        &self,
        spec: &'db TupleSpec<'db>,
    ) -> Result<TupleElementCursor<'db>, Infallible> {
        Ok(TupleElementCursor::new(self.db, spec))
    }

    fn next_tuple_element(
        &self,
        cursor: &mut TupleElementCursor<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(cursor.next())
    }

    fn new_recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<UnionBuilder<'db>, Infallible> {
        Ok(UnionBuilder::new(self.db, env).cycle_recovery(true))
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

    fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Infallible> {
        builder.add_in_place(ty);
        Ok(())
    }

    fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Infallible> {
        Ok(builder.build())
    }

    fn homogeneous_tuple(
        &self,
        env: &ProgramEnvironment<'db>,
        element: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::homogeneous_tuple(self.db, env, element))
    }
}
