//! Child-operation boundaries for the exhaustive type-pair dispatcher.
//!
//! Comparison effects retain the existing checker and builder. A scheduled provider must
//! implement each operation using supervised children; ordinary recursive methods are not
//! implicit defaults. Completion-checked local steps do not supervise recursion inside them.

use std::future::Future;
use std::ops::ControlFlow;

#[cfg(test)]
use super::dependencies::RelationDependencies;
#[cfg(test)]
use super::field_reads::RelationFieldReads;
#[cfg(test)]
use super::guard::{InlineGuard, with_relation_guard};
use super::{EquivalenceChecker, TypeRelationChecker};
#[cfg(test)]
use crate::Db;
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedType, MethodWrapper, MethodWrapperKind,
    SentinelInstance,
};
use crate::types::typevar::TypeVarDomain;
use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, BytesLiteralType, CallableSignature,
    CallableType, ClassLiteral, ClassType, EnumComplementType, EnumLiteralType, FunctionType,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType, NewType,
    NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, RecursiveType,
    SpecialFormType, StringLiteralType, SubclassOfInner, SubclassOfType, Type, TypeAliasType,
    TypeFormType, TypeGuardType, TypeIsType, TypeVarBoundOrConstraints, TypedDictType, UnionType,
};

pub(super) trait PairEffects<'a, 'c, 'db>: Sized {
    type Error;

    async fn type_form_argument(&self, value: TypeFormType<'db>) -> Result<Type<'db>, Self::Error>;
    async fn field_default(
        &self,
        value: FieldInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn field_converter(
        &self,
        value: FieldInstance<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
    async fn method_wrapper_kind(
        &self,
        value: MethodWrapper<'db>,
    ) -> Result<MethodWrapperKind, Self::Error>;
    async fn method_wrapper_type(
        &self,
        value: MethodWrapper<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn partial_wrapped(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> Result<InternedType<'db>, Self::Error>;
    async fn partial_callable(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> Result<CallableType<'db>, Self::Error>;
    async fn interned_type(&self, value: InternedType<'db>) -> Result<Type<'db>, Self::Error>;
    async fn type_is_argument(&self, value: TypeIsType<'db>) -> Result<Type<'db>, Self::Error>;
    async fn type_guard_return(&self, value: TypeGuardType<'db>) -> Result<Type<'db>, Self::Error>;

    async fn step<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Self::Error>;

    async fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    async fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;

    async fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;

    fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_typevar_subclass_relation_to_target(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SubclassOfType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<Option<ConstraintSet<'db, 'c>>, Self::Error>>;

    fn check_newtype_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: NewType<'db>,
        target: NewType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_source_union(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: UnionType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_target_union(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: UnionType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_target_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: IntersectionType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_source_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: IntersectionType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_source_typevar_bounds(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_function_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: FunctionType<'db>,
        target: FunctionType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_bound_method_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: BoundMethodType<'db>,
        target: BoundMethodType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_known_bound_method_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: KnownBoundMethodType<'db>,
        target: KnownBoundMethodType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_callable_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_callable_signature_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: &CallableSignature<'db>,
        target: &CallableSignature<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_callable_source(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: CallableType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_meta_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_typeddict_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_typeddict_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: TypedDictType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_class_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_subclassof_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SubclassOfType<'db>,
        target: SubclassOfType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_nominal_instance_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: NominalInstanceType<'db>,
        target: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_property_instance_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: PropertyInstanceType<'db>,
        target: PropertyInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn when_recursive_types_relate_by_arguments(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_bound_super_pair(
        &self,
        checker: &EquivalenceChecker<'a, 'c, 'db>,
        source: BoundSuperType<'db>,
        target: BoundSuperType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn recursive_type_pair_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn implied_typevar_relation(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn lazy_typevar_upper_constraint(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn lazy_typevar_lower_constraint(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
        source: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn protocol_is_equivalent_to_object(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn same_typevar_occurrence(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: BoundTypeVarInstance<'db>,
        target: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn unfold_recursive(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        recursive: RecursiveType<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn alias_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        alias: TypeAliasType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn union_has_aliases(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn expand_union_aliases(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn subclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        subclass: SubclassOfType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn nominal_has_known_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn class_default_specialization(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<ClassType<'db>, Self::Error>>;

    fn class_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn known_instance_type_form_argument(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: KnownInstanceType<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn special_form_type_form_argument(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        form: SpecialFormType,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn enum_remaining_literals(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        complement: EnumComplementType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn enum_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        complement: EnumComplementType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn same_sentinel(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SentinelInstance<'db>,
        target: SentinelInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn wrapper_matches_nominal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        wrapper: MethodWrapper<'db>,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn lookup_wrapped_function(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn nominal_class_is_known(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn specialize_partial_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn union_contains_dynamic(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn intersection_contains_nondivergent_dynamic(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn union_contains_type(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn intersection_positive_contains(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn intersection_contains_dynamic(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn intersection_negative_contains(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn instance_approximation(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn typevar_is_inferable(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn typevar_is_typevartuple(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn is_exact_tuple_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn is_variadic_exact_tuple_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn unpacked_typevartuple(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn typevar_domain(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<TypeVarDomain, Self::Error>>;

    fn callable_is_gradual_paramspec_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn callable_is_top_paramspec_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn callable_is_bottom_paramspec_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn typevar_constraints(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<&'db [Type<'db>]>, Self::Error>>;

    fn typevar_upper_bound(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn typevar_bound_or_constraints(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>>;

    fn newtype_concrete_base(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        newtype: NewType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn type_is_always_falsy(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn type_is_always_truthy(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn callable_signatures(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<&'db CallableSignature<'db>, Self::Error>>;

    fn function_callable_signatures(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        function: FunctionType<'db>,
    ) -> impl Future<Output = Result<&'db CallableSignature<'db>, Self::Error>>;

    fn known_class_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: KnownClass,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn check_string_literal_nominal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        value: StringLiteralType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_bytes_literal_nominal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        value: BytesLiteralType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn check_enum_instance_literal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        literal: EnumLiteralType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;

    fn literal_fallback_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>>;

    fn callable_runtime_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<Option<KnownClass>, Self::Error>>;

    fn subclass_inner_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        inner: SubclassOfInner<'db>,
    ) -> impl Future<Output = Result<Option<ClassType<'db>>, Self::Error>>;

    fn class_literal_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn class_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn subclass_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        subclass: SubclassOfType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn special_form_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        form: SpecialFormType,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn known_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: KnownInstanceType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    fn property_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        property: PropertyInstanceType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>>;

    async fn guard<F>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> F,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>;
}

pub(super) trait AsyncConstraintSet<'db, 'c>: Sized {
    async fn and_with<'a, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>;
    async fn or_with<'a, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>;
}

impl<'db, 'c> AsyncConstraintSet<'db, 'c> for ConstraintSet<'db, 'c> {
    async fn and_with<'a, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>,
    {
        effects.step(|| self.verify_builder(builder)).await?;
        if self.is_trivially_never_satisfied() {
            return Ok(self);
        }
        let other = other().await?;
        effects
            .combine_constraints(builder, ConstraintFoldKind::All, self, other)
            .await
    }
    async fn or_with<'a, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>,
    {
        effects.step(|| self.verify_builder(builder)).await?;
        if self.is_trivially_always_satisfied() {
            return Ok(self);
        }
        let other = other().await?;
        effects
            .combine_constraints(builder, ConstraintFoldKind::Any, self, other)
            .await
    }
}

pub(super) trait AsyncOptionConstraints<T>: Sized {
    async fn when_some_and_with<'a, 'db, 'c, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        work: impl FnOnce(T) -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>;
    async fn when_none_or_with<'a, 'db, 'c, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        work: impl FnOnce(T) -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>;
}

impl<T> AsyncOptionConstraints<T> for Option<T> {
    async fn when_some_and_with<'a, 'db, 'c, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        work: impl FnOnce(T) -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>,
    {
        match self {
            Some(value) => work(value).await,
            None => {
                effects
                    .step(|| ConstraintSet::from_bool(builder, false))
                    .await
            }
        }
    }
    async fn when_none_or_with<'a, 'db, 'c, E: PairEffects<'a, 'c, 'db>, F>(
        self,
        builder: &'c ConstraintSetBuilder<'db>,
        work: impl FnOnce(T) -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>,
    {
        match self {
            Some(value) => work(value).await,
            None => {
                effects
                    .step(|| ConstraintSet::from_bool(builder, true))
                    .await
            }
        }
    }
}

pub(super) trait AsyncIteratorConstraints: Iterator + Sized {
    async fn when_all_with<'a, 'db, 'c, E: PairEffects<'a, 'c, 'db>, F>(
        mut self,
        builder: &'c ConstraintSetBuilder<'db>,
        mut work: impl FnMut(Self::Item) -> F,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, E::Error>>,
    {
        let mut fold = effects
            .step(|| ConstraintFold::new(builder, ConstraintFoldKind::All))
            .await?;
        while let Some(element) = effects.step(|| self.next()).await? {
            let result = work(element).await?;
            if let ControlFlow::Break(result) = effects.push_constraints(&mut fold, result).await? {
                return Ok(result);
            }
        }
        effects.finish_constraints(&mut fold).await
    }
}

impl<I: Iterator> AsyncIteratorConstraints for I {}

#[cfg(test)]
pub(super) struct InlinePairEffects<'db, D = super::dependencies::OrdinaryDependencies> {
    pub(super) db: &'db dyn Db,
    pub(super) dependencies: D,
}

#[cfg(test)]
impl<'a, 'c, 'db, D: RelationDependencies> PairEffects<'a, 'c, 'db> for InlinePairEffects<'db, D> {
    type Error = D::Error;

    async fn type_form_argument(&self, value: TypeFormType<'db>) -> Result<Type<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).type_form_argument(value)
        })
    }
    async fn field_default(
        &self,
        value: FieldInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).field_default(value)
        })
    }
    async fn field_converter(
        &self,
        value: FieldInstance<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).field_converter(value)
        })
    }
    async fn method_wrapper_kind(
        &self,
        value: MethodWrapper<'db>,
    ) -> Result<MethodWrapperKind, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).method_wrapper_kind(value)
        })
    }
    async fn method_wrapper_type(
        &self,
        value: MethodWrapper<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).method_wrapper_type(value)
        })
    }
    async fn partial_wrapped(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> Result<InternedType<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).partial_wrapped(value)
        })
    }
    async fn partial_callable(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).partial_callable(value)
        })
    }
    async fn interned_type(&self, value: InternedType<'db>) -> Result<Type<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).interned_type(value)
        })
    }
    async fn type_is_argument(&self, value: TypeIsType<'db>) -> Result<Type<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).type_is_argument(value)
        })
    }
    async fn type_guard_return(&self, value: TypeGuardType<'db>) -> Result<Type<'db>, Self::Error> {
        self.dependencies.run(self.db, || {
            RelationFieldReads::new(self.db).type_guard_return(value)
        })
    }

    async fn step<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.dependencies.run(self.db, operation)
    }

    async fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || match kind {
            ConstraintFoldKind::All => left.and(self.db, builder, || right),
            ConstraintFoldKind::Any => left.or(self.db, builder, || right),
        })
    }

    async fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error> {
        self.dependencies.run(self.db, || fold.push(next))
    }

    async fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error> {
        self.dependencies.run(self.db, || fold.finish_borrowed())
    }

    async fn guard<F>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> F,
    ) -> Result<ConstraintSet<'db, 'c>, Self::Error>
    where
        F: Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>>,
    {
        let effects = InlineGuard::new(self.db, &self.dependencies);
        with_relation_guard(checker, source, target, work, &effects).await
    }

    fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.check_type_pair(self.db, source, target)),
        )
    }

    fn check_typevar_subclass_relation_to_target(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SubclassOfType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<Option<ConstraintSet<'db, 'c>>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_typevar_subclass_relation_to_target(self.db, source, target)
        }))
    }

    fn check_newtype_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: NewType<'db>,
        target: NewType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_newtype_pair(self.db, source, target)
        }))
    }

    fn check_source_union(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: UnionType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_source_union(self.db, source, target)
        }))
    }

    fn check_target_union(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: UnionType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_target_union(self.db, source, target)
        }))
    }

    fn check_target_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: IntersectionType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_target_intersection(self.db, source, target)
        }))
    }

    fn check_source_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: IntersectionType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_source_intersection(self.db, source, target)
        }))
    }

    fn check_source_typevar_bounds(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_source_typevar_bounds(self.db, source, target)
        }))
    }

    fn check_function_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: FunctionType<'db>,
        target: FunctionType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_function_pair(self.db, source, target)
        }))
    }

    fn check_bound_method_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: BoundMethodType<'db>,
        target: BoundMethodType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_bound_method_pair(self.db, source, target)
        }))
    }

    fn check_known_bound_method_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: KnownBoundMethodType<'db>,
        target: KnownBoundMethodType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_known_bound_method_pair(self.db, source, target)
        }))
    }

    fn check_callable_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_callable_pair(self.db, source, target)
        }))
    }

    fn check_callable_signature_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: &CallableSignature<'db>,
        target: &CallableSignature<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_callable_signature_pair(self.db, source, target)
        }))
    }

    fn check_callable_source(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: CallableType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_callable_source(self.db, source, target)
        }))
    }

    fn check_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_type_satisfies_protocol(self.db, source, target)
        }))
    }

    fn check_meta_type_satisfies_protocol(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_meta_type_satisfies_protocol(self.db, source, target)
        }))
    }

    fn check_typeddict_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_typeddict_pair(self.db, source, target)
        }))
    }

    fn check_typeddict_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: TypedDictType<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_typeddict_fallback(self.db, source, target)
        }))
    }

    fn check_class_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_class_pair(self.db, source, target)
        }))
    }

    fn check_subclassof_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SubclassOfType<'db>,
        target: SubclassOfType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_subclassof_pair(self.db, source, target)
        }))
    }

    fn check_nominal_instance_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: NominalInstanceType<'db>,
        target: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_nominal_instance_pair(self.db, source, target)
        }))
    }

    fn check_property_instance_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: PropertyInstanceType<'db>,
        target: PropertyInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_property_instance_pair(self.db, source, target)
        }))
    }

    fn when_recursive_types_relate_by_arguments(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.when_recursive_types_relate_by_arguments(self.db, source, target)
        }))
    }

    fn check_bound_super_pair(
        &self,
        checker: &EquivalenceChecker<'a, 'c, 'db>,
        source: BoundSuperType<'db>,
        target: BoundSuperType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_bound_super_pair(self.db, source, target)
        }))
    }

    fn recursive_type_pair_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.recursive_type_pair_fallback(self.db, source, target)
        }))
    }

    fn is_never_satisfied(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            constraints.is_never_satisfied(self.db, checker.env)
        }))
    }
    fn implied_typevar_relation(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.implied_typevar_relation(self.db, source, target)
        }))
    }

    fn lazy_typevar_upper_constraint(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.lazy_typevar_upper_constraint(self.db, typevar, target)
        }))
    }

    fn lazy_typevar_lower_constraint(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
        source: Type<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.lazy_typevar_lower_constraint(self.db, typevar, source)
        }))
    }

    fn protocol_is_equivalent_to_object(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.protocol_is_equivalent_to_object(self.db, protocol)
        }))
    }

    fn same_typevar_occurrence(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: BoundTypeVarInstance<'db>,
        target: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.same_typevar_occurrence(self.db, source, target)
        }))
    }

    fn unfold_recursive(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        recursive: RecursiveType<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.unfold_recursive(self.db, recursive)),
        )
    }

    fn alias_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        alias: TypeAliasType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.alias_value(self.db, alias)),
        )
    }

    fn union_has_aliases(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.union_has_aliases(self.db, union)),
        )
    }

    fn expand_union_aliases(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.expand_union_aliases(self.db, union)),
        )
    }

    fn subclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        subclass: SubclassOfType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.subclass_instance(self.db, subclass)),
        )
    }

    fn nominal_has_known_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.nominal_has_known_class(self.db, instance, class)
        }))
    }

    fn class_default_specialization(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<ClassType<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.class_default_specialization(self.db, class)
        }))
    }

    fn class_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.class_instance(self.db, class)),
        )
    }

    fn known_instance_type_form_argument(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: KnownInstanceType<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.known_instance_type_form_argument(self.db, instance)
        }))
    }

    fn special_form_type_form_argument(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        form: SpecialFormType,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.special_form_type_form_argument(self.db, form)
        }))
    }

    fn enum_remaining_literals(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        complement: EnumComplementType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.enum_remaining_literals(self.db, complement)
        }))
    }

    fn enum_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        complement: EnumComplementType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.enum_intersection(self.db, complement)),
        )
    }

    fn same_sentinel(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SentinelInstance<'db>,
        target: SentinelInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.same_sentinel(self.db, source, target)),
        )
    }

    fn wrapper_matches_nominal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        wrapper: MethodWrapper<'db>,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.wrapper_matches_nominal(self.db, wrapper, instance)
        }))
    }

    fn lookup_wrapped_function(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        target: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.lookup_wrapped_function(self.db, target)),
        )
    }

    fn nominal_class_is_known(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.nominal_class_is_known(self.db, instance, class)
        }))
    }

    fn specialize_partial_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.specialize_partial_instance(self.db, callable)
        }))
    }

    fn union_contains_dynamic(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.union_contains_dynamic(self.db, union)),
        )
    }

    fn intersection_contains_nondivergent_dynamic(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.intersection_contains_nondivergent_dynamic(self.db, intersection)
        }))
    }

    fn union_contains_type(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.union_contains_type(self.db, union, ty)),
        )
    }

    fn intersection_positive_contains(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.intersection_positive_contains(self.db, intersection, ty)
        }))
    }

    fn intersection_contains_dynamic(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.intersection_contains_dynamic(self.db, intersection)
        }))
    }

    fn intersection_negative_contains(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.intersection_negative_contains(self.db, intersection, ty)
        }))
    }

    fn instance_approximation(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.instance_approximation(self.db, ty)),
        )
    }

    fn typevar_is_inferable(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.typevar_is_inferable(self.db, typevar)),
        )
    }

    fn typevar_is_typevartuple(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.typevar_is_typevartuple(self.db, typevar)
        }))
    }

    fn is_exact_tuple_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.is_exact_tuple_instance(self.db, ty)),
        )
    }

    fn is_variadic_exact_tuple_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.is_variadic_exact_tuple_instance(self.db, ty)
        }))
    }

    fn unpacked_typevartuple(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.unpacked_typevartuple(self.db, typevar)),
        )
    }

    fn typevar_domain(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<TypeVarDomain, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.typevar_domain(self.db, typevar)),
        )
    }

    fn callable_is_gradual_paramspec_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.callable_is_gradual_paramspec_value(self.db, callable)
        }))
    }

    fn callable_is_top_paramspec_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.callable_is_top_paramspec_value(self.db, callable)
        }))
    }

    fn callable_is_bottom_paramspec_value(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.callable_is_bottom_paramspec_value(self.db, callable)
        }))
    }

    fn typevar_constraints(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<&'db [Type<'db>]>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.typevar_constraints(self.db, typevar)),
        )
    }

    fn typevar_upper_bound(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.typevar_upper_bound(self.db, typevar)),
        )
    }

    fn typevar_bound_or_constraints(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.typevar_bound_or_constraints(self.db, typevar)
        }))
    }

    fn newtype_concrete_base(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        newtype: NewType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.newtype_concrete_base(self.db, newtype)),
        )
    }

    fn type_is_always_falsy(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.type_is_always_falsy(self.db, ty)),
        )
    }

    fn type_is_always_truthy(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.type_is_always_truthy(self.db, ty)),
        )
    }

    fn callable_signatures(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<&'db CallableSignature<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.callable_signatures(self.db, callable)),
        )
    }

    fn function_callable_signatures(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        function: FunctionType<'db>,
    ) -> impl Future<Output = Result<&'db CallableSignature<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.function_callable_signatures(self.db, function)
        }))
    }

    fn known_class_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: KnownClass,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.known_class_instance(self.db, class)),
        )
    }

    fn check_string_literal_nominal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        value: StringLiteralType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_string_literal_nominal(self.db, value, instance)
        }))
    }

    fn check_bytes_literal_nominal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        value: BytesLiteralType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_bytes_literal_nominal(self.db, value, instance)
        }))
    }

    fn check_enum_instance_literal(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        literal: EnumLiteralType<'db>,
    ) -> impl Future<Output = Result<ConstraintSet<'db, 'c>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.check_enum_instance_literal(self.db, source, literal)
        }))
    }

    fn literal_fallback_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.literal_fallback_instance(self.db, source)
        }))
    }

    fn callable_runtime_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> impl Future<Output = Result<Option<KnownClass>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.callable_runtime_class(self.db, callable)
        }))
    }

    fn subclass_inner_class(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        inner: SubclassOfInner<'db>,
    ) -> impl Future<Output = Result<Option<ClassType<'db>>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.subclass_inner_class(self.db, inner)),
        )
    }

    fn class_literal_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.class_literal_metaclass_instance(self.db, class)
        }))
    }

    fn class_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(
            self.dependencies
                .run(self.db, || checker.class_metaclass_instance(self.db, class)),
        )
    }

    fn subclass_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        subclass: SubclassOfType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.subclass_metaclass_instance(self.db, subclass)
        }))
    }

    fn special_form_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        form: SpecialFormType,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.special_form_instance_fallback(self.db, form)
        }))
    }

    fn known_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: KnownInstanceType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.known_instance_fallback(self.db, instance)
        }))
    }

    fn property_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        property: PropertyInstanceType<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        std::future::ready(self.dependencies.run(self.db, || {
            checker.property_instance_fallback(self.db, property)
        }))
    }
}

#[cfg(test)]
mod tests;
