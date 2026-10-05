use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::GenericIntersection;
use crate::types::generics::{Specialization, specialization_variance};
use crate::types::mro::MroIterator;
use crate::types::tuple::{FixedLengthTuple, TupleSpec, VariableSegment};
use crate::types::visitor::contains_growing_type;
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, ClassBase, ClassType, GenericAlias, GenericContext,
    IntersectionType, KnownClass, MaterializationKind, StaticClassLiteral, Type, TypeVarVariance,
    UnionType,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct GenericIntersectionFacts;

pub(super) struct OrdinaryGenericIntersectionEffects<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
}

impl<'a, 'db> OrdinaryGenericIntersectionEffects<'a, 'db> {
    pub(super) fn new(db: &'db dyn Db, env: &'a ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousGenericIntersectionEffects)]
    // Collection owners remain borrowed across rejectable operations. Creating a cursor or
    // collection prepays its cleanup, including when a later semantic dependency refuses.
    pub(in crate::types) trait GenericIntersectionEffects<'db> {
        type Error;
        type Variables;
        type Types;
        type Mro;

        #[operation(child)]
        async fn dynamic_generalization(&self, general: Type<'db>, specific: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn base_top(&self, base: Type<'db>, subclass: Type<'db>) -> Result<Option<GenericIntersection<'db>>, Self::Error>;
        #[operation(source)]
        async fn has_dynamic(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn class_specialization(&self, ty: Type<'db>) -> Result<Option<(StaticClassLiteral<'db>, Specialization<'db>)>, Self::Error>;
        #[operation(source)]
        async fn materialization(&self, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(source)]
        async fn known_class(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(source)]
        async fn tuple(&self, specialization: Specialization<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
        #[operation(local)]
        async fn has_fixed_elements(&self, tuple: &'db TupleSpec<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn fixed_tuple(&self, tuple: &'db TupleSpec<'db>) -> Result<Option<&'db FixedLengthTuple<Type<'db>>>, Self::Error>;
        #[operation(local)]
        async fn same_tuple_length(&self, first: &'db FixedLengthTuple<Type<'db>>, second: &'db FixedLengthTuple<Type<'db>>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn tuple_elements(&self, tuple: &'db FixedLengthTuple<Type<'db>>) -> Result<Self::Types, Self::Error>;
        #[operation(source)]
        async fn homogeneous_tuple(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn heterogeneous_tuple_intersections(&self, specific: &'db FixedLengthTuple<Type<'db>>, general: &'db FixedLengthTuple<Type<'db>>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn generic_context(&self, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<Self::Variables, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, variables: &mut Self::Variables) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Self::Error>;
        #[operation(source)]
        async fn specialization_types(&self, specialization: Specialization<'db>) -> Result<Self::Types, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, types: &mut Self::Types) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn types_equal(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn variance(&self, variable: BoundTypeVarInstance<'db>) -> Result<TypeVarVariance, Self::Error>;
        #[operation(source)]
        async fn intersection(&self, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union(&self, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_types(&self) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn reserve_types(&self, types: &mut Vec<Type<'db>>, variables: &Self::Variables, first: &Self::Types, second: &Self::Types) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn copy_specialization_types(&self, specialization: Specialization<'db>, types: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn append_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn get_type(&self, types: &[Type<'db>], index: usize) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn replace_type(&self, types: &mut [Type<'db>], index: usize, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn specialize(&self, context: GenericContext<'db>, types: &mut Vec<Type<'db>>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(source)]
        async fn apply_specialization(&self, class: StaticClassLiteral<'db>, specialization: Specialization<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn identity_specialization(&self, class: StaticClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn mro(&self, class: ClassType<'db>) -> Result<Self::Mro, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_ancestor(&self, mro: &mut Self::Mro) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(source)]
        async fn alias_origin(&self, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn alias_specialization(&self, alias: GenericAlias<'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(source)]
        async fn contains_growing_type(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn fully_static(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn variable_identity(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn top_materialization(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl GenericIntersectionFacts {
        fn is_variable_or_newtype(&self, ty: Type<'_>) -> bool {
            matches!(ty, Type::TypeVar(_) | Type::NewTypeInstance(_))
        }
        fn nominal_or_protocol(&self, ty: Type<'_>) -> bool {
            matches!(ty, Type::NominalInstance(_) | Type::ProtocolInstance(_))
        }
        fn nominal(&self, ty: Type<'_>) -> bool { ty.is_nominal_instance() }
        fn same_class(&self, first: StaticClassLiteral<'_>, second: StaticClassLiteral<'_>) -> bool { first == second }
        fn same_specialization(&self, first: Specialization<'_>, second: Specialization<'_>) -> bool { first == second }
        fn is_known(&self, value: Option<KnownClass>, expected: KnownClass) -> bool { value == Some(expected) }
        fn has_materialization(&self, value: Option<MaterializationKind>) -> bool { value.is_some() }
        fn materialization_is(&self, value: Option<MaterializationKind>, expected: MaterializationKind) -> bool { value == Some(expected) }
        fn homogeneous_element<'db>(&self, variable: &crate::types::tuple::VariableLengthTuple<Type<'db>, VariableSegment<'db>>) -> Option<Type<'db>> {
            variable.variable().homogeneous_type()
        }
        fn dynamic(&self, ty: Type<'_>) -> bool { ty.is_non_divergent_dynamic() }
        fn never(&self, ty: Type<'_>) -> bool { ty.is_never() }
        fn object<'db>(&self) -> Type<'db> { Type::object() }
        fn same_identity(&self, first: BoundTypeVarIdentity<'_>, second: BoundTypeVarIdentity<'_>) -> bool { first == second }
    }

    #[synchronous(generic_gradual_intersection_sync)]
    #[capabilities(effects = GenericIntersectionEffects)]
    #[passive_values(GenericIntersection::Simplified)]
    pub(in crate::types) async fn generic_gradual_intersection_with<'db, E: GenericIntersectionEffects<'db>>(
        left: Type<'db>, right: Type<'db>, effects: &E,
    ) -> Result<Option<GenericIntersection<'db>>, E::Error> {
        if let Some(result) = effects.dynamic_generalization(left, right).await? {
            return Ok(Some(GenericIntersection::Simplified(result)));
        }
        if let Some(result) = effects.dynamic_generalization(right, left).await? {
            return Ok(Some(GenericIntersection::Simplified(result)));
        }
        if let Some(result) = effects.base_top(left, right).await? {
            return Ok(Some(result));
        }
        effects.base_top(right, left).await
    }

    /// Intersect two specializations of the same generic class if `general` only differs from
    /// `specific` by using dynamic types.
    ///
    /// For example, `list[Any]` dynamically generalizes `list[int]`, while `list[str]` does not.
    #[synchronous(dynamic_generalization_intersection_sync)]
    #[capabilities(effects = GenericIntersectionEffects, facts = GenericIntersectionFacts)]
    #[passive_values(KnownClass::Tuple)]
    pub(in crate::types) async fn dynamic_generalization_intersection_with<'db, E: GenericIntersectionEffects<'db>>(
        general: Type<'db>, specific: Type<'db>, facts: GenericIntersectionFacts, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        // Fast path to avoid performance regressions.
        if !effects.has_dynamic(general).await?
            || facts.is_variable_or_newtype(general)
            || facts.is_variable_or_newtype(specific)
        {
            return Ok(None);
        }

        let general_selection = effects.class_specialization(general).await?;
        let specific_selection = effects.class_specialization(specific).await?;
        let (Some((general_class, general_specialization)), Some((specific_class, specific_specialization))) =
            (general_selection, specific_selection)
        else {
            return Ok(None);
        };

        // Top and bottom materializations are not gradual types.
        if !facts.same_class(general_class, specific_class)
            || facts.same_specialization(general_specialization, specific_specialization)
            || facts.has_materialization(effects.materialization(general_specialization).await?)
            || facts.has_materialization(effects.materialization(specific_specialization).await?)
        {
            return Ok(None);
        }

        if facts.is_known(effects.known_class(general_class).await?, KnownClass::Tuple) {
            let Some(general_tuple) = effects.tuple(general_specialization).await? else {
                return Ok(None);
            };
            let Some(specific_tuple) = effects.tuple(specific_specialization).await? else {
                return Ok(None);
            };

            if let (TupleSpec::Variable(general_variable), TupleSpec::Variable(specific_variable)) =
                (general_tuple, specific_tuple)
            {
                if effects.has_fixed_elements(general_tuple).await?
                    || effects.has_fixed_elements(specific_tuple).await?
                    || effects.has_dynamic(specific).await?
                {
                    return Ok(None);
                }

                let Some(general_element) = facts.homogeneous_element(general_variable) else {
                    return Ok(None);
                };
                if !facts.dynamic(general_element) {
                    return Ok(None);
                }
                let Some(specific_element) = facts.homogeneous_element(specific_variable) else {
                    return Ok(None);
                };
                let element = effects.intersection(specific_element, general_element).await?;
                return Ok(Some(effects.homogeneous_tuple(element).await?));
            }

            let Some(general_tuple) = effects.fixed_tuple(general_tuple).await? else {
                return Ok(None);
            };
            let Some(specific_tuple) = effects.fixed_tuple(specific_tuple).await? else {
                return Ok(None);
            };
            if !effects.same_tuple_length(general_tuple, specific_tuple).await? {
                return Ok(None);
            }
            let mut general_elements = effects.tuple_elements(general_tuple).await?;
            let mut specific_elements = effects.tuple_elements(specific_tuple).await?;
            #[cursor_loop]
            while let Some(general_element) = effects.next_type(&mut general_elements).await? {
                let Some(specific_element) = effects.next_type(&mut specific_elements).await? else {
                    break;
                };
                if !effects.types_equal(general_element, specific_element).await?
                    && !facts.dynamic(general_element)
                {
                    return Ok(None);
                }
            }
            if effects.has_dynamic(specific).await? {
                return Ok(None);
            }

            return Ok(Some(effects.heterogeneous_tuple_intersections(specific_tuple, general_tuple).await?));
        }

        let generic_context = effects.generic_context(general_specialization).await?;
        let mut variables = effects.variables(generic_context).await?;
        let mut general_types = effects.specialization_types(general_specialization).await?;
        let mut specific_types = effects.specialization_types(specific_specialization).await?;
        #[cursor_loop]
        while let Some(_entry) = effects.next_variable(&mut variables).await? {
            let Some(general) = effects.next_type(&mut general_types).await? else { break; };
            let Some(specific) = effects.next_type(&mut specific_types).await? else { break; };
            if !effects.types_equal(general, specific).await? && !facts.dynamic(general) {
                return Ok(None);
            }
        }

        #[passive_state]
        let mut has_variant_replacement = false;
        let mut variables = effects.variables(generic_context).await?;
        let mut general_types = effects.specialization_types(general_specialization).await?;
        let mut specific_types = effects.specialization_types(specific_specialization).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut variables).await? {
            let (_, typevar) = entry;
            let Some(general) = effects.next_type(&mut general_types).await? else { break; };
            let Some(specific) = effects.next_type(&mut specific_types).await? else { break; };
            if !effects.types_equal(general, specific).await?
                && matches!(effects.variance(typevar).await?, TypeVarVariance::Covariant | TypeVarVariance::Contravariant)
            {
                has_variant_replacement = true;
                break;
            }
        }

        if !has_variant_replacement {
            return Ok(Some(specific));
        }

        if effects.has_dynamic(specific).await? {
            return Ok(None);
        }

        let mut variables = effects.variables(generic_context).await?;
        let mut general_types = effects.specialization_types(general_specialization).await?;
        let mut specific_types = effects.specialization_types(specific_specialization).await?;
        let mut types = effects.new_types().await?;
        #[passive_state]
        let mut reserved = false;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut variables).await? {
            let (_, typevar) = entry;
            let Some(general) = effects.next_type(&mut general_types).await? else { break; };
            let Some(specific) = effects.next_type(&mut specific_types).await? else { break; };
            let ty = if effects.types_equal(general, specific).await? {
                specific
            } else {
                match effects.variance(typevar).await? {
                    TypeVarVariance::Covariant => effects.intersection(specific, general).await?,
                    TypeVarVariance::Contravariant => effects.union(specific, general).await?,
                    TypeVarVariance::Invariant | TypeVarVariance::Bivariant => specific,
                }
            };
            if !reserved {
                effects.reserve_types(&mut types, &variables, &general_types, &specific_types).await?;
                reserved = true;
            }
            effects.append_type(&mut types, ty).await?;
        }
        let specialization = effects.specialize(generic_context, &mut types).await?;
        let class = effects.apply_specialization(general_class, specialization).await?;
        Ok(Some(effects.instance(class).await?))
    }

    /// Intersect a fully static nominal base with a generic subclass.
    ///
    /// The subclass's identity MRO determines which subclass type variables specialize the base.
    /// Restricting those variables by the base's variance preserves invariant subclass
    /// materializations instead of incorrectly collapsing, for example,
    /// `Sequence[int] & Top[list[Any]]` to `list[int]`.
    /// Subclass arguments that do not specialize the base retain their gradualness.
    #[synchronous(base_top_intersection_sync)]
    #[capabilities(effects = GenericIntersectionEffects, facts = GenericIntersectionFacts)]
    #[passive_values(KnownClass::Iterable, KnownClass::Iterator, KnownClass::Tuple, GenericIntersection::Recursive, GenericIntersection::Simplified, MaterializationKind::Bottom, MaterializationKind::Top)]
    pub(in crate::types) async fn base_top_intersection_with<'db, E: GenericIntersectionEffects<'db>>(
        base: Type<'db>, subclass: Type<'db>, facts: GenericIntersectionFacts, effects: &E,
    ) -> Result<Option<GenericIntersection<'db>>, E::Error> {
        if !facts.nominal_or_protocol(base)
            || !facts.nominal_or_protocol(subclass)
            || effects.has_dynamic(base).await?
        {
            return Ok(None);
        }

        let Some((base_class, base_specialization)) = effects.class_specialization(base).await? else {
            return Ok(None);
        };
        let Some((subclass_class, subclass_specialization)) = effects.class_specialization(subclass).await? else {
            return Ok(None);
        };

        // As a deliberately unsound exception, allow `Iterable` as the base when the subclass is
        // nominal or is the `Iterator` protocol. We assume containers and iterators obey their
        // behavioral contracts, including agreement between iteration and indexing.
        let is_iterable_special_case = facts.is_known(effects.known_class(base_class).await?, KnownClass::Iterable)
            && (facts.nominal(subclass)
                || facts.is_known(effects.known_class(subclass_class).await?, KnownClass::Iterator));
        if !is_iterable_special_case && (!facts.nominal(base) || !facts.nominal(subclass)) {
            return Ok(None);
        }

        if facts.same_class(base_class, subclass_class) {
            return Ok(None);
        }

        let identity = effects.identity_specialization(subclass_class).await?;
        let mut mro = effects.mro(identity).await?;
        #[passive_state]
        let mut inherited_specialization = None;
        #[cursor_loop]
        while let Some(ancestor) = effects.next_ancestor(&mut mro).await? {
            if let ClassBase::Class(ClassType::Generic(alias)) = ancestor
                && facts.same_class(effects.alias_origin(alias).await?, base_class)
            {
                inherited_specialization = Some(effects.alias_specialization(alias).await?);
                break;
            }
        }
        let Some(inherited_specialization) = inherited_specialization else {
            return Ok(None);
        };

        // Inspect lazy attributes only after establishing that the classes are related. Expanding a
        // recursive generic alias or member can re-enter intersection simplification with ever-growing
        // type arguments. Exact recursive specializations can still be checked.
        if effects.contains_growing_type(base).await? {
            return Ok(Some(GenericIntersection::Recursive));
        }
        if facts.has_materialization(effects.materialization(base_specialization).await?)
            || facts.materialization_is(effects.materialization(subclass_specialization).await?, MaterializationKind::Bottom)
            || !effects.fully_static(base).await?
        {
            return Ok(None);
        }

        let subclass_context = effects.generic_context(subclass_specialization).await?;
        let mut types = effects.new_types().await?;
        effects.copy_specialization_types(subclass_specialization, &mut types).await?;
        #[passive_state]
        let mut changed = false;

        let base_context = effects.generic_context(base_specialization).await?;
        let mut base_variables = effects.variables(base_context).await?;
        let mut base_types = effects.specialization_types(base_specialization).await?;
        let mut inherited_types = effects.specialization_types(inherited_specialization).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next_variable(&mut base_variables).await? {
            let (_, base_typevar) = entry;
            let Some(base_type) = effects.next_type(&mut base_types).await? else { break; };
            let Some(inherited_type) = effects.next_type(&mut inherited_types).await? else { break; };
            let Type::TypeVar(subclass_typevar) = inherited_type else {
                return Ok(None);
            };
            let mut subclass_variables = effects.variables(subclass_context).await?;
            #[passive_state]
            let mut subclass_index = None;
            #[cursor_loop]
            while let Some(entry) = effects.next_variable(&mut subclass_variables).await? {
                let (index, typevar) = entry;
                let identity = effects.variable_identity(typevar).await?;
                let subclass_identity = effects.variable_identity(subclass_typevar).await?;
                if facts.same_identity(identity, subclass_identity) {
                    subclass_index = Some(index);
                    break;
                }
            }
            let Some(subclass_index) = subclass_index else {
                return Ok(None);
            };
            let subclass_type = effects.get_type(&types, subclass_index).await?;

            if effects.types_equal(subclass_type, base_type).await? {
                continue;
            }

            let is_top_generalization = match effects.variance(subclass_typevar).await? {
                TypeVarVariance::Covariant => effects.types_equal(subclass_type, facts.object()).await?,
                TypeVarVariance::Contravariant => facts.never(subclass_type),
                TypeVarVariance::Invariant => {
                    facts.materialization_is(effects.materialization(subclass_specialization).await?, MaterializationKind::Top)
                        && facts.dynamic(subclass_type)
                }
                TypeVarVariance::Bivariant => false,
            };

            if !is_top_generalization {
                return Ok(None);
            }

            let replacement = match effects.variance(base_typevar).await? {
                TypeVarVariance::Covariant => effects.intersection(subclass_type, base_type).await?,
                TypeVarVariance::Contravariant => effects.union(subclass_type, base_type).await?,
                TypeVarVariance::Invariant => base_type,
                TypeVarVariance::Bivariant => return Ok(None),
            };
            effects.replace_type(&mut types, subclass_index, replacement).await?;
            changed = true;
        }

        if !changed {
            return Ok(None);
        }

        if facts.is_known(effects.known_class(subclass_class).await?, KnownClass::Tuple) {
            let Some(tuple) = effects.tuple(subclass_specialization).await? else {
                return Ok(None);
            };
            // A homogeneous tuple would lose the shape of any fixed prefix or suffix.
            if effects.has_fixed_elements(tuple).await? {
                return Ok(None);
            }
            let TupleSpec::Variable(variable) = tuple else {
                return Ok(None);
            };
            let Some(_element) = facts.homogeneous_element(variable) else {
                return Ok(None);
            };
            let first = effects.get_type(&types, 0).await?;
            return Ok(Some(GenericIntersection::Simplified(effects.homogeneous_tuple(first).await?)));
        }

        let specialization = effects.specialize(subclass_context, &mut types).await?;
        let class = effects.apply_specialization(subclass_class, specialization).await?;
        let specialized = effects.instance(class).await?;
        let result = if facts.materialization_is(effects.materialization(subclass_specialization).await?, MaterializationKind::Top) {
            effects.top_materialization(specialized).await?
        } else {
            specialized
        };
        Ok(Some(GenericIntersection::Simplified(result)))
    }
}

impl<'db> SynchronousGenericIntersectionEffects<'db>
    for OrdinaryGenericIntersectionEffects<'_, 'db>
{
    type Error = Infallible;
    type Variables = std::iter::Enumerate<
        std::iter::Copied<
            ordermap::map::Values<'db, BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
        >,
    >;
    type Types = std::iter::Copied<std::slice::Iter<'db, Type<'db>>>;
    type Mro = MroIterator<'db>;

    fn dynamic_generalization(
        &self,
        general: Type<'db>,
        specific: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        dynamic_generalization_intersection_sync(general, specific, GenericIntersectionFacts, self)
    }

    fn base_top(
        &self,
        base: Type<'db>,
        subclass: Type<'db>,
    ) -> Result<Option<GenericIntersection<'db>>, Self::Error> {
        base_top_intersection_sync(base, subclass, GenericIntersectionFacts, self)
    }

    fn has_dynamic(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.has_dynamic(self.db, self.env))
    }

    fn class_specialization(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Specialization<'db>)>, Self::Error> {
        Ok(ty.class_specialization(self.db, self.env))
    }

    fn materialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(self.db))
    }

    fn known_class(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }

    fn tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        Ok(specialization.tuple(self.db))
    }

    fn has_fixed_elements(&self, tuple: &'db TupleSpec<'db>) -> Result<bool, Self::Error> {
        Ok(tuple.fixed_elements().next().is_some())
    }

    fn fixed_tuple(
        &self,
        tuple: &'db TupleSpec<'db>,
    ) -> Result<Option<&'db FixedLengthTuple<Type<'db>>>, Self::Error> {
        Ok(tuple.as_fixed_length())
    }

    fn same_tuple_length(
        &self,
        first: &'db FixedLengthTuple<Type<'db>>,
        second: &'db FixedLengthTuple<Type<'db>>,
    ) -> Result<bool, Self::Error> {
        Ok(first.len() == second.len())
    }

    fn tuple_elements(
        &self,
        tuple: &'db FixedLengthTuple<Type<'db>>,
    ) -> Result<Self::Types, Self::Error> {
        Ok(tuple.elements_slice().iter().copied())
    }

    fn homogeneous_tuple(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::homogeneous_tuple(self.db, self.env, ty))
    }

    fn heterogeneous_tuple_intersections(
        &self,
        specific: &'db FixedLengthTuple<Type<'db>>,
        general: &'db FixedLengthTuple<Type<'db>>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::heterogeneous_tuple(
            self.db,
            self.env,
            specific
                .iter_all_elements()
                .zip(general.iter_all_elements())
                .map(|(specific, general)| {
                    IntersectionType::from_two_elements(self.db, self.env, specific, general)
                }),
        ))
    }

    fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(self.db))
    }

    fn variables(&self, context: GenericContext<'db>) -> Result<Self::Variables, Self::Error> {
        Ok(context.variables(self.db).enumerate())
    }

    fn next_variable(
        &self,
        variables: &mut Self::Variables,
    ) -> Result<Option<(usize, BoundTypeVarInstance<'db>)>, Self::Error> {
        Ok(variables.next())
    }

    fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Self::Types, Self::Error> {
        Ok(specialization.types(self.db).iter().copied())
    }

    fn next_type(&self, types: &mut Self::Types) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(types.next())
    }

    fn types_equal(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error> {
        Ok(first == second)
    }

    fn variance(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, Self::Error> {
        Ok(specialization_variance(self.db, variable))
    }

    fn intersection(&self, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(IntersectionType::from_two_elements(
            self.db, self.env, first, second,
        ))
    }

    fn union(&self, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(
            self.db, self.env, first, second,
        ))
    }

    fn new_types(&self) -> Result<Vec<Type<'db>>, Self::Error> {
        Ok(Vec::new())
    }

    fn reserve_types(
        &self,
        types: &mut Vec<Type<'db>>,
        variables: &Self::Variables,
        first: &Self::Types,
        second: &Self::Types,
    ) -> Result<(), Self::Error> {
        // As with collecting the mapped iterator, reserve after producing the first element.
        let remaining = variables.len().min(first.len()).min(second.len());
        types.reserve(remaining.saturating_add(1));
        Ok(())
    }

    fn copy_specialization_types(
        &self,
        specialization: Specialization<'db>,
        types: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        *types = specialization.types(self.db).to_vec();
        Ok(())
    }

    fn append_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error> {
        types.push(ty);
        Ok(())
    }

    fn get_type(&self, types: &[Type<'db>], index: usize) -> Result<Type<'db>, Self::Error> {
        Ok(types[index])
    }

    fn replace_type(
        &self,
        types: &mut [Type<'db>],
        index: usize,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        types[index] = ty;
        Ok(())
    }

    fn specialize(
        &self,
        context: GenericContext<'db>,
        types: &mut Vec<Type<'db>>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(context.specialize(self.db, std::mem::take(types)))
    }

    fn apply_specialization(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.apply_optional_specialization(self.db, Some(specialization)))
    }

    fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(self.db, self.env, class))
    }

    fn identity_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.identity_specialization(self.db))
    }

    fn mro(&self, class: ClassType<'db>) -> Result<Self::Mro, Self::Error> {
        Ok(class.iter_mro(self.db))
    }

    fn next_ancestor(&self, mro: &mut Self::Mro) -> Result<Option<ClassBase<'db>>, Self::Error> {
        Ok(mro.next())
    }

    fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(self.db))
    }

    fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(alias.specialization(self.db))
    }

    fn contains_growing_type(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(contains_growing_type(self.db, self.env, ty))
    }

    fn fully_static(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.is_fully_static(self.db, self.env))
    }

    fn variable_identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Self::Error> {
        Ok(variable.identity(self.db))
    }

    fn top_materialization(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.top_materialization(self.db, self.env))
    }
}
