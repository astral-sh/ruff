use itertools::Itertools;
use rustc_hash::FxHashSet;

use super::TypeRelationChecker;
use super::target_union::{InlineTargetUnionEffects, union_has_aliases_sync};
use crate::Db;
use crate::types::constraints::{ConstraintSet, OptionConstraintsExtension};
use crate::types::enums::is_single_member_enum;
use crate::types::known_instance::{MethodWrapper, SentinelInstance};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::TupleType;
use crate::types::typevar::TypeVarDomain;
use crate::types::{
    BoundTypeVarInstance, BytesLiteralType, CallableSignature, CallableType, ClassBase,
    ClassLiteral, ClassType, EnumComplementType, EnumLiteralType, FunctionType, IntersectionType,
    KnownClass, KnownInstanceType, MemberLookupPolicy, NewType, NominalInstanceType,
    PropertyInstanceType, ProtocolInstanceType, RecursiveType, SpecialFormType, StringLiteralType,
    SubclassOfInner, SubclassOfType, Type, TypeAliasType, TypeVarBoundOrConstraints, UnionType,
};

impl<'a, 'c, 'db> TypeRelationChecker<'a, 'c, 'db> {
    #[inline]
    pub(super) fn implied_typevar_relation(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.given
            .implies_subtype_of(db, self.env, self.constraints, source, target)
    }

    #[inline]
    pub(super) fn lazy_typevar_upper_constraint(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let upper = if self.relation.is_subtyping() {
            target.bottom_materialization(db, self.env)
        } else {
            target
        };
        ConstraintSet::constrain_typevar_upper_bound(db, self.env, self.constraints, typevar, upper)
    }

    #[inline]
    pub(super) fn lazy_typevar_lower_constraint(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        source: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let lower = if self.relation.is_subtyping() {
            source.top_materialization(db, self.env)
        } else {
            source
        };
        ConstraintSet::constrain_typevar_lower_bound(db, self.env, self.constraints, typevar, lower)
    }

    #[inline]
    pub(super) fn protocol_is_equivalent_to_object(
        &self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
    ) -> bool {
        protocol.is_equivalent_to_object(db)
    }

    #[inline]
    pub(super) fn same_typevar_occurrence(
        &self,
        db: &'db dyn Db,
        source: BoundTypeVarInstance<'db>,
        target: BoundTypeVarInstance<'db>,
    ) -> bool {
        source.is_same_typevar_as(db, target)
    }

    #[inline]
    pub(super) fn unfold_recursive(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> Option<Type<'db>> {
        recursive.unfold(db, self.env).into_unfolded()
    }

    #[inline]
    pub(super) fn alias_value(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) -> Type<'db> {
        alias.value_type(db)
    }

    #[inline]
    pub(super) fn union_has_aliases(&self, db: &'db dyn Db, union: UnionType<'db>) -> bool {
        union_has_aliases_sync(union, &InlineTargetUnionEffects::new(db, self))
            .unwrap_or_else(|never| match never {})
    }

    #[inline]
    pub(super) fn expand_union_aliases(&self, db: &'db dyn Db, union: UnionType<'db>) -> Type<'db> {
        union.expand_aliases(db, self.env)
    }

    #[inline]
    pub(super) fn subclass_instance(
        &self,
        db: &'db dyn Db,
        subclass: SubclassOfType<'db>,
    ) -> Type<'db> {
        subclass.to_instance(db, self.env)
    }

    #[inline]
    pub(super) fn nominal_has_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> bool {
        instance.has_known_class(db, class)
    }

    #[inline]
    pub(super) fn class_default_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> ClassType<'db> {
        class.default_specialization(db)
    }

    #[inline]
    pub(super) fn class_instance(&self, db: &'db dyn Db, class: ClassType<'db>) -> Type<'db> {
        Type::instance(db, self.env, class)
    }

    #[inline]
    pub(super) fn known_instance_type_form_argument(
        &self,
        db: &'db dyn Db,
        instance: KnownInstanceType<'db>,
    ) -> Option<Type<'db>> {
        instance.type_form_argument(db, self.env)
    }

    #[inline]
    pub(super) fn special_form_type_form_argument(
        &self,
        db: &'db dyn Db,
        form: SpecialFormType,
    ) -> Option<Type<'db>> {
        form.type_form_argument(db, self.env)
    }

    #[inline]
    pub(super) fn enum_remaining_literals(
        &self,
        db: &'db dyn Db,
        complement: EnumComplementType<'db>,
    ) -> Type<'db> {
        complement.remaining_literal_union(db, self.env)
    }

    #[inline]
    pub(super) fn enum_intersection(
        &self,
        db: &'db dyn Db,
        complement: EnumComplementType<'db>,
    ) -> Type<'db> {
        complement.to_intersection(db, self.env)
    }

    #[inline]
    pub(super) fn same_sentinel(
        &self,
        db: &'db dyn Db,
        source: SentinelInstance<'db>,
        target: SentinelInstance<'db>,
    ) -> bool {
        source.is_same_sentinel(db, target)
    }

    #[inline]
    pub(super) fn wrapper_matches_nominal(
        &self,
        db: &'db dyn Db,
        wrapper: MethodWrapper<'db>,
        instance: NominalInstanceType<'db>,
    ) -> bool {
        instance.class(db, self.env).is_known(db, wrapper.class(db))
    }

    #[inline]
    pub(super) fn lookup_wrapped_function(
        &self,
        db: &'db dyn Db,
        target: Type<'db>,
    ) -> Option<Type<'db>> {
        target
            .member_lookup_with_policy(
                db,
                self.env,
                "__func__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
            .ignore_possibly_undefined()
    }

    #[inline]
    pub(super) fn nominal_class_is_known(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> bool {
        instance.class(db, self.env).is_known(db, class)
    }

    #[inline]
    pub(super) fn specialize_partial_instance(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Type<'db> {
        callable.into_functools_partial_instance(db, self.env)
    }

    #[inline]
    pub(super) fn union_contains_dynamic(&self, db: &'db dyn Db, union: UnionType<'db>) -> bool {
        union.elements(db).iter().any(Type::is_dynamic)
    }

    #[inline]
    pub(super) fn intersection_contains_nondivergent_dynamic(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> bool {
        intersection
            .positive(db)
            .iter()
            .any(Type::is_non_divergent_dynamic)
    }

    #[inline]
    pub(super) fn union_contains_type(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
        ty: Type<'db>,
    ) -> bool {
        union.elements(db).contains(&ty)
    }

    #[inline]
    pub(super) fn intersection_positive_contains(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> bool {
        intersection.positive(db).contains(&ty)
    }

    #[inline]
    pub(super) fn intersection_contains_dynamic(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> bool {
        intersection.positive(db).iter().any(Type::is_dynamic)
    }

    #[inline]
    pub(super) fn intersection_negative_contains(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> bool {
        intersection.negative(db).contains(&ty)
    }

    #[inline]
    pub(super) fn instance_approximation(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Option<Type<'db>> {
        ty.to_instance_approximation(db, self.env)
    }

    #[inline]
    pub(super) fn typevar_is_inferable(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> bool {
        typevar.is_inferable(db, self.inferable)
    }

    #[inline]
    pub(super) fn typevar_is_typevartuple(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> bool {
        typevar.is_typevartuple(db)
    }

    #[inline]
    pub(super) fn is_exact_tuple_instance(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        ty.exact_tuple_instance_spec(db).is_some()
    }

    #[inline]
    pub(super) fn is_variadic_exact_tuple_instance(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        ty.exact_tuple_instance_spec(db)
            .is_some_and(|spec| spec.is_variadic())
    }

    #[inline]
    pub(super) fn unpacked_typevartuple(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Type<'db> {
        Type::tuple(TupleType::unpacked_typevartuple(db, self.env, typevar))
    }

    #[inline]
    pub(super) fn typevar_domain(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> TypeVarDomain {
        typevar.domain(db)
    }

    #[inline]
    pub(super) fn callable_is_gradual_paramspec_value(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> bool {
        Self::is_gradual_paramspec_value(db, callable)
    }

    #[inline]
    pub(super) fn callable_is_top_paramspec_value(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> bool {
        callable.is_top_paramspec_value(db)
    }

    #[inline]
    pub(super) fn callable_is_bottom_paramspec_value(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> bool {
        callable.is_bottom_paramspec_value(db)
    }

    #[inline]
    pub(super) fn typevar_constraints(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Option<&'db [Type<'db>]> {
        typevar.typevar(db).constraints(db, self.env)
    }

    #[inline]
    pub(super) fn typevar_upper_bound(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Option<Type<'db>> {
        typevar.typevar(db).upper_bound(db, self.env)
    }

    #[inline]
    pub(super) fn typevar_bound_or_constraints(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Option<TypeVarBoundOrConstraints<'db>> {
        typevar.typevar(db).bound_or_constraints(db, self.env)
    }

    #[inline]
    pub(super) fn newtype_concrete_base(
        &self,
        db: &'db dyn Db,
        newtype: NewType<'db>,
    ) -> Type<'db> {
        newtype.concrete_base_type(db)
    }

    #[inline]
    pub(super) fn type_is_always_falsy(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        ty.bool(db, self.env).is_always_false()
    }

    #[inline]
    pub(super) fn type_is_always_truthy(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        ty.bool(db, self.env).is_always_true()
    }

    #[inline]
    pub(super) fn callable_signatures(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> &'db CallableSignature<'db> {
        callable.signatures(db)
    }

    #[inline]
    pub(super) fn function_callable_signatures(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> &'db CallableSignature<'db> {
        function.into_callable_type(db).signatures(db)
    }

    #[inline]
    pub(super) fn known_class_instance(&self, db: &'db dyn Db, class: KnownClass) -> Type<'db> {
        class.to_instance(db, self.env)
    }

    #[inline]
    pub(super) fn check_string_literal_nominal(
        &self,
        db: &'db dyn Db,
        value: StringLiteralType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let target_class = instance.class(db, self.env);

        if target_class.is_known(db, KnownClass::Str) {
            return self.always();
        }

        if let Some(sequence_class) = KnownClass::Sequence.try_to_class_literal(db, self.env)
            && !sequence_class
                .iter_mro(db, None)
                .filter_map(ClassBase::into_class)
                .map(|class| class.class_literal(db))
                .contains(&target_class.class_literal(db))
        {
            return self.never();
        }

        let chars: FxHashSet<char> = value.value(db).chars().collect();

        let spec = match chars.len() {
            0 => Type::Never,
            1 => Type::single_char_string_literal(db, *chars.iter().next().unwrap()),
            _ => {
                // Optimisation: since we know this union will only include string-literal types,
                // avoid eagerly creating string-literal types when unnecessary, and avoid going
                // via the union-builder.
                let union_elements: Box<[Type<'db>]> = chars
                    .iter()
                    .map(|c| Type::single_char_string_literal(db, *c))
                    .collect();
                Type::Union(UnionType::new(db, union_elements, RecursivelyDefined::No))
            }
        };

        KnownClass::Sequence
            .to_specialized_class_type(db, self.env, &[spec])
            .when_some_and(db, self.constraints, |sequence| {
                self.check_class_pair(db, sequence, target_class)
            })
    }

    #[inline]
    pub(super) fn check_bytes_literal_nominal(
        &self,
        db: &'db dyn Db,
        value: BytesLiteralType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let target_class = instance.class(db, self.env);

        if target_class.is_known(db, KnownClass::Bytes) {
            return self.always();
        }

        if let Some(sequence_class) = KnownClass::Sequence.try_to_class_literal(db, self.env)
            && !sequence_class
                .iter_mro(db, None)
                .filter_map(ClassBase::into_class)
                .map(|class| class.class_literal(db))
                .contains(&target_class.class_literal(db))
        {
            return self.never();
        }

        let ints: FxHashSet<i64> = value
            .value(db)
            .iter()
            .map(|byte| i64::from(*byte))
            .collect();

        let spec = match ints.len() {
            0 => Type::Never,
            1 => Type::int_literal(*ints.iter().next().unwrap()),
            _ => {
                let union_elements: Box<[Type<'db>]> =
                    ints.iter().map(|int| Type::int_literal(*int)).collect();
                Type::Union(UnionType::new(db, union_elements, RecursivelyDefined::No))
            }
        };

        KnownClass::Sequence
            .to_specialized_class_type(db, self.env, &[spec])
            .when_some_and(db, self.constraints, |sequence| {
                self.check_class_pair(db, sequence, target_class)
            })
    }

    #[inline]
    pub(super) fn check_enum_instance_literal(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        literal: EnumLiteralType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if literal.enum_class_instance(db, self.env) != source {
            self.never()
        } else {
            ConstraintSet::from_bool(
                self.constraints,
                is_single_member_enum(db, literal.enum_class(db)),
            )
        }
    }

    #[inline]
    pub(super) fn literal_fallback_instance(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
    ) -> Option<Type<'db>> {
        source.literal_fallback_instance(db, self.env)
    }

    #[inline]
    pub(super) fn callable_runtime_class(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Option<KnownClass> {
        callable.runtime_class(db)
    }

    #[inline]
    pub(super) fn subclass_inner_class(
        &self,
        db: &'db dyn Db,
        inner: SubclassOfInner<'db>,
    ) -> Option<ClassType<'db>> {
        inner.into_class(db, self.env)
    }

    #[inline]
    pub(super) fn class_literal_metaclass_instance(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Type<'db> {
        class.metaclass_instance_type(db, self.env)
    }

    #[inline]
    pub(super) fn class_metaclass_instance(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Type<'db> {
        class.metaclass_instance_type(db, self.env)
    }

    #[inline]
    pub(super) fn subclass_metaclass_instance(
        &self,
        db: &'db dyn Db,
        subclass: SubclassOfType<'db>,
    ) -> Type<'db> {
        subclass.to_metaclass_instance(db, self.env)
    }

    #[inline]
    pub(super) fn special_form_instance_fallback(
        &self,
        db: &'db dyn Db,
        form: SpecialFormType,
    ) -> Type<'db> {
        form.instance_fallback(db, self.env)
    }

    #[inline]
    pub(super) fn known_instance_fallback(
        &self,
        db: &'db dyn Db,
        instance: KnownInstanceType<'db>,
    ) -> Type<'db> {
        instance.instance_fallback(db, self.env)
    }

    #[inline]
    pub(super) fn property_instance_fallback(
        &self,
        db: &'db dyn Db,
        property: PropertyInstanceType<'db>,
    ) -> Type<'db> {
        property.instance_fallback(db, self.env)
    }
}
