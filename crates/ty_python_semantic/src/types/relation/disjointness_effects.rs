//! Dependencies of the complete disjointness dispatcher.
//!
//! Providers retain the caller's checker and comparison owners. Unsupported controlled
//! dependencies must refuse before entering an ordinary recursive or mutating operation.

use std::future::Future;

use super::DisjointnessChecker;
use crate::types::constraints::ConstraintSet;
use crate::types::enums::EnumClassLiteral;
use crate::types::known_instance::{FunctoolsPartialInstance, MethodWrapper, SentinelInstance};
use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType, ClassLiteral, ClassType,
    EnumComplementType, EnumLiteralType, FunctionType, GenericAlias, InternedType,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueType,
    NewType, NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, RecursiveType,
    SpecialFormType, StaticClassLiteral, SubclassOfType, Type, TypeAliasType, TypeFormType,
    TypedDictType, UnionType,
};

pub(in crate::types) trait DisjointnessEffects<'a, 'c, 'db> {
    type Error;

    async fn disjointness_left_alias(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        alias: TypeAliasType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_right_alias(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        alias: TypeAliasType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_left_enum_complement(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        complement: EnumComplementType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_right_enum_complement(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
        complement: EnumComplementType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_subclass_typeform(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        subclass_of: SubclassOfType<'db>,
        typeform: TypeFormType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_typevar_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        type_var: BoundTypeVarInstance<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_typevar_instance(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        type_var: BoundTypeVarInstance<'db>,
        instance: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_typevar_bounds(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        tvar: BoundTypeVarInstance<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_union(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_intersections(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_intersection: IntersectionType<'db>,
        right_intersection: IntersectionType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_left_intersection(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_right_intersection(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_property_instance_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: PropertyInstanceType<'db>,
        right: PropertyInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_interned_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: InternedType<'db>,
        right: InternedType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_method_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: BoundMethodType<'db>,
        right: BoundMethodType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_wrappers(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_wrapper: MethodWrapper<'db>,
        right_wrapper: MethodWrapper<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_partials(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_partial: FunctoolsPartialInstance<'db>,
        right_partial: FunctoolsPartialInstance<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn type_is_always_falsy(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn type_is_always_truthy(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_protocols(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_proto: ProtocolInstanceType<'db>,
        right_proto: ProtocolInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_protocol_special_form(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        special_form: SpecialFormType,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_protocol_known_instance(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        known_instance: KnownInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_protocol_members(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        ty: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_protocol_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        nominal: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_protocol_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_alias_specializations(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left_alias: GenericAlias<'db>,
        right_alias: GenericAlias<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_class_alias(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
        alias_b: GenericAlias<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_subclass_class(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        class_a: ClassType<'db>,
        class_b: ClassLiteral<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_subclass_alias(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        class_a: ClassType<'db>,
        alias_b: GenericAlias<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_subclassof_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: SubclassOfType<'db>,
        right: SubclassOfType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_subclass_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        subclass_of_ty: SubclassOfType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_special_form_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        special_form: SpecialFormType,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_known_instance_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        known_instance: KnownInstanceType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_literal_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        literal: LiteralValueType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_bool_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_newtype_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        newtype: NewType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_class_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_alias_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        alias: GenericAlias<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_function_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        function: FunctionType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_callable_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        class: KnownClass,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_bound_method_fallback(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        a: BoundMethodType<'db>,
        b: BoundMethodType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_bound_method_functions(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        a: BoundMethodType<'db>,
        b: BoundMethodType<'db>,
        a_function: FunctionType<'db>,
        b_function: FunctionType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_bound_method_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_known_method_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        method: KnownBoundMethodType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_descriptor_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_callable_final_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        nominal: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_module_nominal(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_nominal_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_i: NominalInstanceType<'db>,
        right_i: NominalInstanceType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_newtype_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: NewType<'db>,
        right: NewType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_property_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        property: PropertyInstanceType<'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_slot_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_bound_super_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: BoundSuperType<'db>,
        right: BoundSuperType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_super_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_typeddicts(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_td: TypedDictType<'db>,
        right_td: TypedDictType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_typeddict_other(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_left_recursive(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_recursive: RecursiveType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_right_recursive(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        right_recursive: RecursiveType<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_transposed_typevar(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        subclass_of: SubclassOfType<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;

    async fn disjointness_instance_approximation(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn disjointness_typevar_is_inferable(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left_tvar: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_same_typevar(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left_tvar: BoundTypeVarInstance<'db>,
        right_tvar: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_negative_contains_typevar(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        tvar: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_same_wrapper_kind(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left_wrapper: MethodWrapper<'db>,
        right_wrapper: MethodWrapper<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_nominal_is_final(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        nominal: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_callable_runtime_class(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error>;

    async fn disjointness_known_instance(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        other_class: KnownClass,
    ) -> Result<Type<'db>, Self::Error>;

    async fn disjointness_bound_function(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        a: BoundMethodType<'db>,
    ) -> Result<Option<FunctionType<'db>>, Self::Error>;

    async fn disjointness_function_names_differ(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        a_function: FunctionType<'db>,
        b_function: FunctionType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_same_sentinel(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left_sentinel: SentinelInstance<'db>,
        right_sentinel: SentinelInstance<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_enum_class(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: EnumLiteralType<'db>,
    ) -> Result<EnumClassLiteral<'db>, Self::Error>;

    async fn disjointness_enum_aliases_known(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        class: EnumClassLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_literal_kinds_differ(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: LiteralValueType<'db>,
        right: LiteralValueType<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_types_differ(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn disjointness_alias_origin(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left_alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;

    async fn disjointness_boolean(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        value: bool,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_type_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn check_type_pair_impl(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn disjointness_clear_context(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
    ) -> Result<(), Self::Error>;

    async fn disjointness_has_context(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
    ) -> Result<bool, Self::Error>;

    async fn is_always_satisfied(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Self::Error>;

    async fn or<F>(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        value: ConstraintSet<'db, 'c>,
        next: impl FnOnce() -> F,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;
}
