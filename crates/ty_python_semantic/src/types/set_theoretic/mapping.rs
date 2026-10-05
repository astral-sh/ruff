//! Ordered union and intersection mapping with lazy union reconstruction.

use std::convert::Infallible;
use std::slice;

use super::builder::intersection_insertion::Elements;
use super::{
    IntersectionBuilder, IntersectionType, NegativeIntersectionElements,
    NegativeIntersectionElementsIterator, RecursivelyDefined, UnionBuilder, UnionType,
};
use crate::types::{
    ApplyTypeMappingVisitor, PromotionKind, PromotionMode, Type, TypeContext, TypeMapping,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

pub(in crate::types) struct SetMappingFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSetMappingEffects)]
    pub(in crate::types) trait SetMappingEffects<'db> {
        type Error;
        type Union;
        type Intersection;

        #[operation(source)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_union(&self, elements: &mut slice::Iter<'_, Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn union_recursively_defined(&self, union: UnionType<'db>) -> Result<RecursivelyDefined, Self::Error>;
        #[operation(local)]
        async fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<Self::Union, Self::Error>;
        #[operation(local)]
        async fn new_structural_union(&self, capacity: usize) -> Result<Self::Union, Self::Error>;
        #[operation(child)]
        async fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish_union(&self, builder: Self::Union, recursively_defined: RecursivelyDefined) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn positive_elements(&self, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(source)]
        async fn negative_elements(&self, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_intersection(&self, elements: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn new_intersection(&self, env: &ProgramEnvironment<'db>) -> Result<Self::Intersection, Self::Error>;
        #[operation(local)]
        async fn new_structural_intersection(&self, positive_capacity: usize) -> Result<Self::Intersection, Self::Error>;
        #[operation(child)]
        async fn add_positive(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn add_negative(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn finish_intersection(&self, builder: Self::Intersection) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl SetMappingFacts {
        fn environment<'a, 'db>(&self, visitor: &'a ApplyTypeMappingVisitor<'_, 'db>) -> &'a ProgramEnvironment<'db> { visitor.env }
        fn structural(&self, mapping: &TypeMapping<'_, '_>) -> bool { mapping.is_structural() }
        fn keep_negative(&self, mapping: &TypeMapping<'_, '_>) -> bool {
            !matches!(mapping, TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular))
        }
        fn flip<'a, 'db>(&self, mapping: &TypeMapping<'a, 'db>) -> TypeMapping<'a, 'db> { mapping.flip() }
        fn changed(&self, original: Type<'_>, mapped: Type<'_>) -> bool { original != mapped }
        fn cursor<'a, 'db>(&self, elements: &'a [Type<'db>]) -> slice::Iter<'a, Type<'db>> { elements.iter() }
        fn len(&self, elements: &[Type<'_>]) -> usize { elements.len() }
        fn intersection_len(&self, elements: &Elements<'_>) -> usize {
            match elements {
                Elements::Positive(iter) => iter.len(),
                Elements::Negative(NegativeIntersectionElementsIterator::EmptyOrOne(element)) => usize::from(element.is_some()),
                Elements::Negative(NegativeIntersectionElementsIterator::Multiple(iter)) => iter.len(),
            }
        }
        fn prefix<'a, 'db>(&self, elements: &'a [Type<'db>], remaining: &slice::Iter<'_, Type<'db>>) -> slice::Iter<'a, Type<'db>> {
            // The cursor has just consumed the first changed member of this same slice.
            elements[..elements.len() - remaining.len() - 1].iter()
        }
    }

    #[synchronous(map_union_sync)]
    #[capabilities(effects = SetMappingEffects, facts = SetMappingFacts)]
    #[passive_values(Type::Union)]
    pub(in crate::types) async fn map_union_with<'db, E: SetMappingEffects<'db>>(
        db: &'db dyn Db, union: UnionType<'db>, mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E, facts: SetMappingFacts,
    ) -> Result<Type<'db>, E::Error> {
        let elements = effects.union_elements(union).await?;
        let mut cursor = facts.cursor(elements);
        if facts.structural(mapping) {
            let mut builder = effects.new_structural_union(facts.len(elements)).await?;
            #[cursor_loop]
            while let Some(ty) = effects.next_union(&mut cursor).await? {
                let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                effects.union_add(&mut builder, mapped).await?;
            }
            let recursively_defined = effects.union_recursively_defined(union).await?;
            return effects.finish_union(builder, recursively_defined).await;
        }
        #[cursor_loop]
        while let Some(ty) = effects.next_union(&mut cursor).await? {
            let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
            if facts.changed(ty, mapped) {
                let mut builder = effects.new_union(facts.environment(visitor)).await?;
                let mut prefix = facts.prefix(elements, &cursor);
                #[cursor_loop]
                while let Some(previous) = effects.next_union(&mut prefix).await? {
                    effects.union_add(&mut builder, previous).await?;
                }
                effects.union_add(&mut builder, mapped).await?;
                #[cursor_loop]
                while let Some(ty) = effects.next_union(&mut cursor).await? {
                    let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
                    effects.union_add(&mut builder, mapped).await?;
                }
                let recursively_defined = effects.union_recursively_defined(union).await?;
                return effects.finish_union(builder, recursively_defined).await;
            }
        }
        Ok(Type::Union(union))
    }

    #[synchronous(map_intersection_sync)]
    #[capabilities(effects = SetMappingEffects, facts = SetMappingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn map_intersection_with<'db, E: SetMappingEffects<'db>>(
        db: &'db dyn Db, intersection: IntersectionType<'db>, mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E, facts: SetMappingFacts,
    ) -> Result<Type<'db>, E::Error> {
        let (mut builder, mut positive) = if facts.structural(mapping) {
            let positive = effects.positive_elements(intersection).await?;
            let builder = effects.new_structural_intersection(facts.intersection_len(&positive)).await?;
            (builder, positive)
        } else {
            let builder = effects.new_intersection(facts.environment(visitor)).await?;
            let positive = effects.positive_elements(intersection).await?;
            (builder, positive)
        };
        #[cursor_loop]
        while let Some(ty) = effects.next_intersection(&mut positive).await? {
            let mapped = effects.map_type(db, ty, mapping, tcx, visitor).await?;
            effects.add_positive(&mut builder, mapped).await?;
        }
        // Regular promotion should remove negative contributions from intersections,
        // so we don't preserve them here when regular promotion is enabled.
        if facts.keep_negative(mapping) {
            let mut negative = effects.negative_elements(intersection).await?;
            #[cursor_loop]
            while let Some(ty) = effects.next_intersection(&mut negative).await? {
                let mapping = facts.flip(mapping);
                let mapped = effects.map_type(db, ty, &mapping, tcx, visitor).await?;
                effects.add_negative(&mut builder, mapped).await?;
            }
        }
        effects.finish_intersection(builder).await
    }
}

pub(super) struct OrdinarySetMapping<'db> {
    pub(super) db: &'db dyn Db,
}

pub(super) enum MappedUnion<'db> {
    Semantic(UnionBuilder<'db>),
    Structural(Vec<Type<'db>>),
}

pub(super) enum MappedIntersection<'db> {
    Semantic(IntersectionBuilder<'db>),
    Structural {
        positive: FxOrderSet<Type<'db>>,
        negative: NegativeIntersectionElements<'db>,
    },
}

impl<'db> SynchronousSetMappingEffects<'db> for OrdinarySetMapping<'db> {
    type Error = Infallible;
    type Union = MappedUnion<'db>;
    type Intersection = MappedIntersection<'db>;

    fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(union.elements(self.db))
    }
    fn next_union(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(elements.next().copied())
    }
    fn union_recursively_defined(
        &self,
        union: UnionType<'db>,
    ) -> Result<RecursivelyDefined, Infallible> {
        Ok(union.recursively_defined(self.db))
    }
    fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<Self::Union, Infallible> {
        Ok(MappedUnion::Semantic(
            UnionBuilder::new(self.db, env).unpack_aliases(false),
        ))
    }
    fn new_structural_union(&self, capacity: usize) -> Result<Self::Union, Infallible> {
        Ok(MappedUnion::Structural(Vec::with_capacity(capacity)))
    }
    fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> Result<(), Infallible> {
        match builder {
            MappedUnion::Semantic(builder) => {
                builder.add_in_place(ty);
            }
            MappedUnion::Structural(elements) => elements.push(ty),
        }
        Ok(())
    }
    fn finish_union(
        &self,
        builder: Self::Union,
        recursively_defined: RecursivelyDefined,
    ) -> Result<Type<'db>, Infallible> {
        Ok(match builder {
            MappedUnion::Semantic(builder) => {
                builder.or_recursively_defined(recursively_defined).build()
            }
            MappedUnion::Structural(elements) => Type::Union(UnionType::new(
                self.db,
                elements.into_boxed_slice(),
                recursively_defined,
            )),
        })
    }
    fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Infallible> {
        Ok(Elements::Positive(intersection.positive(self.db).iter()))
    }
    fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Infallible> {
        Ok(Elements::Negative(intersection.negative(self.db).iter()))
    }
    fn next_intersection(
        &self,
        elements: &mut Elements<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(match elements {
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
    }
    fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Self::Intersection, Infallible> {
        Ok(MappedIntersection::Semantic(IntersectionBuilder::new(
            self.db, env,
        )))
    }
    fn new_structural_intersection(
        &self,
        positive_capacity: usize,
    ) -> Result<Self::Intersection, Infallible> {
        Ok(MappedIntersection::Structural {
            positive: FxOrderSet::with_capacity_and_hasher(positive_capacity, Default::default()),
            negative: NegativeIntersectionElements::default(),
        })
    }
    fn add_positive(
        &self,
        builder: &mut Self::Intersection,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        match builder {
            MappedIntersection::Semantic(builder) => {
                builder.add_positive_in_place(ty);
            }
            MappedIntersection::Structural { positive, .. } => {
                positive.insert(ty);
            }
        }
        Ok(())
    }
    fn add_negative(
        &self,
        builder: &mut Self::Intersection,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        match builder {
            MappedIntersection::Semantic(builder) => {
                builder.add_negative_in_place(ty);
            }
            MappedIntersection::Structural { negative, .. } => {
                negative.insert(ty);
            }
        }
        Ok(())
    }
    fn finish_intersection(&self, builder: Self::Intersection) -> Result<Type<'db>, Infallible> {
        Ok(match builder {
            MappedIntersection::Semantic(builder) => builder.build(),
            MappedIntersection::Structural { positive, negative } => {
                Type::Intersection(IntersectionType::new(self.db, positive, negative))
            }
        })
    }
    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_db::testing::assert_function_query_was_not_run_by_name;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::set_theoretic::builder::MAX_NON_RECURSIVE_UNION_LITERALS;
    use crate::types::{KnownClass, KnownInstanceType, MaterializationKind, TypeFormType};

    #[derive(Debug, PartialEq)]
    enum Event<'db> {
        Child(Type<'db>, Option<MaterializationKind>, Option<Type<'db>>),
        NewUnion,
        UnionAdd(Type<'db>),
        RecursionFlag,
        FinishUnion,
        NewIntersection,
        PositiveRead,
        NegativeRead,
        PositiveAdd(Type<'db>),
        NegativeAdd(Type<'db>),
        FinishIntersection,
    }

    struct ObservedMapping<'db> {
        ordinary: OrdinarySetMapping<'db>,
        events: RefCell<Vec<Event<'db>>>,
        replacement: Option<(Type<'db>, Type<'db>)>,
    }

    impl<'db> ObservedMapping<'db> {
        fn new(db: &'db dyn Db) -> Self {
            Self {
                ordinary: OrdinarySetMapping { db },
                events: RefCell::default(),
                replacement: None,
            }
        }

        fn record(&self, event: Event<'db>) {
            self.events.borrow_mut().push(event);
        }
    }

    impl<'db> SynchronousSetMappingEffects<'db> for ObservedMapping<'db> {
        type Error = Infallible;
        type Union = MappedUnion<'db>;
        type Intersection = MappedIntersection<'db>;

        fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Infallible> {
            self.ordinary.union_elements(union)
        }
        fn next_union(
            &self,
            elements: &mut slice::Iter<'_, Type<'db>>,
        ) -> Result<Option<Type<'db>>, Infallible> {
            self.ordinary.next_union(elements)
        }
        fn union_recursively_defined(
            &self,
            union: UnionType<'db>,
        ) -> Result<RecursivelyDefined, Infallible> {
            self.record(Event::RecursionFlag);
            self.ordinary.union_recursively_defined(union)
        }
        fn new_union(&self, env: &ProgramEnvironment<'db>) -> Result<Self::Union, Infallible> {
            self.record(Event::NewUnion);
            self.ordinary.new_union(env)
        }
        fn new_structural_union(&self, capacity: usize) -> Result<Self::Union, Infallible> {
            self.ordinary.new_structural_union(capacity)
        }
        fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> Result<(), Infallible> {
            self.record(Event::UnionAdd(ty));
            self.ordinary.union_add(builder, ty)
        }
        fn finish_union(
            &self,
            builder: Self::Union,
            flag: RecursivelyDefined,
        ) -> Result<Type<'db>, Infallible> {
            self.record(Event::FinishUnion);
            self.ordinary.finish_union(builder, flag)
        }
        fn positive_elements(
            &self,
            intersection: IntersectionType<'db>,
        ) -> Result<Elements<'db>, Infallible> {
            self.record(Event::PositiveRead);
            self.ordinary.positive_elements(intersection)
        }
        fn negative_elements(
            &self,
            intersection: IntersectionType<'db>,
        ) -> Result<Elements<'db>, Infallible> {
            self.record(Event::NegativeRead);
            self.ordinary.negative_elements(intersection)
        }
        fn next_intersection(
            &self,
            elements: &mut Elements<'db>,
        ) -> Result<Option<Type<'db>>, Infallible> {
            self.ordinary.next_intersection(elements)
        }
        fn new_intersection(
            &self,
            env: &ProgramEnvironment<'db>,
        ) -> Result<Self::Intersection, Infallible> {
            self.record(Event::NewIntersection);
            self.ordinary.new_intersection(env)
        }
        fn new_structural_intersection(
            &self,
            capacity: usize,
        ) -> Result<Self::Intersection, Infallible> {
            self.ordinary.new_structural_intersection(capacity)
        }
        fn add_positive(
            &self,
            builder: &mut Self::Intersection,
            ty: Type<'db>,
        ) -> Result<(), Infallible> {
            self.record(Event::PositiveAdd(ty));
            self.ordinary.add_positive(builder, ty)
        }
        fn add_negative(
            &self,
            builder: &mut Self::Intersection,
            ty: Type<'db>,
        ) -> Result<(), Infallible> {
            self.record(Event::NegativeAdd(ty));
            self.ordinary.add_negative(builder, ty)
        }
        fn finish_intersection(
            &self,
            builder: Self::Intersection,
        ) -> Result<Type<'db>, Infallible> {
            self.record(Event::FinishIntersection);
            self.ordinary.finish_intersection(builder)
        }
        fn map_type(
            &self,
            _db: &'db dyn Db,
            ty: Type<'db>,
            mapping: &TypeMapping<'_, 'db>,
            tcx: TypeContext<'db>,
            _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        ) -> Result<Type<'db>, Infallible> {
            let kind = match mapping {
                TypeMapping::Materialize(kind) => Some(*kind),
                _ => None,
            };
            self.record(Event::Child(ty, kind, tcx.annotation));
            Ok(match self.replacement {
                Some((original, mapped)) if ty == original => mapped,
                _ => ty,
            })
        }
    }

    #[test]
    fn unchanged_union_does_not_create_a_builder_or_read_reconstruction_metadata() {
        let db = setup_db();
        let env = db.program_environment();
        let elements = [Type::int_literal(1), Type::int_literal(2)];
        let union = UnionType::new(
            &db,
            elements.to_vec().into_boxed_slice(),
            RecursivelyDefined::Yes,
        );
        let effects = ObservedMapping::new(&db);
        let actual = map_union_sync(
            &db,
            union,
            &TypeMapping::Materialize(MaterializationKind::Top),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
            &effects,
            SetMappingFacts,
        )
        .unwrap();
        assert_eq!(actual, Type::Union(union));
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Child(elements[0], Some(MaterializationKind::Top), None),
                Event::Child(elements[1], Some(MaterializationKind::Top), None)
            ]
        );
    }

    #[test]
    fn first_change_copies_the_prefix_once_before_mapping_the_suffix() {
        let db = setup_db();
        let env = db.program_environment();
        let elements = [
            Type::int_literal(1),
            Type::int_literal(2),
            Type::int_literal(3),
        ];
        let mapped = Type::int_literal(4);
        let union = UnionType::new(
            &db,
            elements.to_vec().into_boxed_slice(),
            RecursivelyDefined::No,
        );
        let mut effects = ObservedMapping::new(&db);
        effects.replacement = Some((elements[1], mapped));
        let annotation = Some(Type::object());
        let actual = map_union_sync(
            &db,
            union,
            &TypeMapping::Materialize(MaterializationKind::Top),
            TypeContext::new(annotation),
            &ApplyTypeMappingVisitor::new(&env),
            &effects,
            SetMappingFacts,
        )
        .unwrap();
        assert_eq!(
            actual.expect_union().elements(&db),
            &[elements[0], mapped, elements[2]]
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Child(elements[0], Some(MaterializationKind::Top), annotation),
                Event::Child(elements[1], Some(MaterializationKind::Top), annotation),
                Event::NewUnion,
                Event::UnionAdd(elements[0]),
                Event::UnionAdd(mapped),
                Event::Child(elements[2], Some(MaterializationKind::Top), annotation),
                Event::UnionAdd(elements[2]),
                Event::RecursionFlag,
                Event::FinishUnion,
            ]
        );
    }

    #[test]
    fn alias_preserving_reconstruction_rebuilds_the_prefix_for_literal_widening() {
        let db = setup_db();
        let env = db.program_environment();
        let marker = KnownClass::Str.to_instance(&db, &env);
        let limit = i64::try_from(MAX_NON_RECURSIVE_UNION_LITERALS).unwrap();
        let union =
            UnionType::from_elements(&db, &env, (0..limit).map(Type::int_literal).chain([marker]))
                .expect_union();
        let mut effects = ObservedMapping::new(&db);
        effects.replacement = Some((marker, Type::int_literal(limit)));
        let actual = map_union_sync(
            &db,
            union,
            &TypeMapping::ReplaceParameterDefaults,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
            &effects,
            SetMappingFacts,
        )
        .unwrap();
        assert_eq!(actual, KnownClass::Int.to_instance(&db, &env));
    }

    #[test]
    fn reconstructed_union_keeps_recursion_flags_from_both_parent_and_children() {
        let db = setup_db();
        let env = db.program_environment();
        let form = TypeFormType::from_type_expression(&db, Type::bool_literal(true));
        for (parent, child) in [
            (RecursivelyDefined::Yes, RecursivelyDefined::No),
            (RecursivelyDefined::No, RecursivelyDefined::Yes),
        ] {
            let union = UnionType::new(&db, vec![Type::any(), form].into_boxed_slice(), parent);
            let replacement = Type::Union(UnionType::new(
                &db,
                vec![Type::int_literal(1), Type::int_literal(2)].into_boxed_slice(),
                child,
            ));
            let mut effects = ObservedMapping::new(&db);
            effects.replacement = Some((Type::any(), replacement));
            let actual = map_union_sync(
                &db,
                union,
                &TypeMapping::Materialize(MaterializationKind::Top),
                TypeContext::default(),
                &ApplyTypeMappingVisitor::new(&env),
                &effects,
                SetMappingFacts,
            )
            .unwrap();
            assert_eq!(
                actual.expect_union().recursively_defined(&db),
                RecursivelyDefined::Yes
            );
        }
    }

    #[test]
    fn changed_union_preserves_alias_members_without_evaluating_their_bodies() {
        let mut db = setup_db();
        db.write_dedented("/src/alias.py", "type Alias = int")
            .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/alias.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
            global_symbol(&db, file, "Alias").place.expect_type()
        else {
            panic!("expected a type alias");
        };
        let alias = Type::TypeAlias(alias);
        let union = UnionType::new(
            &db,
            vec![alias, Type::any()].into_boxed_slice(),
            RecursivelyDefined::No,
        );
        let mapped = Type::bool_literal(true);
        let mut effects = ObservedMapping::new(&db);
        effects.replacement = Some((Type::any(), mapped));
        let actual = map_union_sync(
            &db,
            union,
            &TypeMapping::Materialize(MaterializationKind::Top),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
            &effects,
            SetMappingFacts,
        )
        .unwrap();
        assert_eq!(actual.expect_union().elements(&db), &[alias, mapped]);
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "raw_value_type", None, &events);
    }

    #[test]
    fn intersection_maps_each_sign_in_order_and_flips_only_negative_children() {
        let db = setup_db();
        let env = db.program_environment();
        let positive = [Type::any(), Type::object()];
        let negative = [Type::bool_literal(true), Type::bool_literal(false)];
        let intersection = IntersectionType::new(
            &db,
            FxOrderSet::from_iter(positive),
            NegativeIntersectionElements::Multiple(FxOrderSet::from_iter(negative)),
        );
        let annotation = Some(Type::object());
        for kind in [MaterializationKind::Top, MaterializationKind::Bottom] {
            let effects = ObservedMapping::new(&db);
            map_intersection_sync(
                &db,
                intersection,
                &TypeMapping::Materialize(kind),
                TypeContext::new(annotation),
                &ApplyTypeMappingVisitor::new(&env),
                &effects,
                SetMappingFacts,
            )
            .unwrap();
            assert_eq!(
                *effects.events.borrow(),
                [
                    Event::NewIntersection,
                    Event::PositiveRead,
                    Event::Child(positive[0], Some(kind), annotation),
                    Event::PositiveAdd(positive[0]),
                    Event::Child(positive[1], Some(kind), annotation),
                    Event::PositiveAdd(positive[1]),
                    Event::NegativeRead,
                    Event::Child(negative[0], Some(kind.flip()), annotation),
                    Event::NegativeAdd(negative[0]),
                    Event::Child(negative[1], Some(kind.flip()), annotation),
                    Event::NegativeAdd(negative[1]),
                    Event::FinishIntersection,
                ]
            );
        }
    }

    #[test]
    fn regular_promotion_does_not_read_or_map_negative_members() {
        let db = setup_db();
        let env = db.program_environment();
        let positive = Type::object();
        let intersection = IntersectionType::new(
            &db,
            FxOrderSet::from_iter([positive]),
            NegativeIntersectionElements::Single(Type::any()),
        );
        let effects = ObservedMapping::new(&db);
        let actual = map_intersection_sync(
            &db,
            intersection,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
            &effects,
            SetMappingFacts,
        )
        .unwrap();
        assert_eq!(actual, positive);
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::NewIntersection,
                Event::PositiveRead,
                Event::Child(positive, None, None),
                Event::PositiveAdd(positive),
                Event::FinishIntersection
            ]
        );
    }
}
