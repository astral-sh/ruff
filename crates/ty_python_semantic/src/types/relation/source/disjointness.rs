//! Disjointness borrows the same constraint and visitor owners as ordinary comparisons.

use std::future::Future;

use salsa::execution_probe::{BorrowOrCopy, RunError, RunResult};

use super::disjoint_intersection::BorrowedDisjointIntersection;
use super::retained::PairChildren;
use super::{BorrowedPairs, RelationSourceEffects, RelationSourceOperation};
use crate::types::constraints::{ConstraintFoldKind, ConstraintSet};
use crate::types::enums::EnumClassLiteral;
use crate::types::known_instance::{FunctoolsPartialInstance, MethodWrapper, SentinelInstance};
use crate::types::relation::disjoint_intersection::{
    DisjointIntersectionOperands, check_disjoint_intersection_with,
};
use crate::types::relation::disjointness_effects::DisjointnessEffects;
use crate::types::relation::pair_effects::PairEffects;
use crate::types::relation::{DisjointnessChecker, RelationFieldReads};
use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableType, ClassLiteral, ClassType,
    EnumComplementType, EnumLiteralType, FunctionType, GenericAlias, InternedType,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueType,
    NewType, NominalInstanceType, PropertyInstanceType, ProtocolInstanceType, RecursiveType,
    SpecialFormType, StaticClassLiteral, SubclassOfType, Type, TypeAliasType, TypeFormType,
    TypedDictType, UnionType,
};

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    pub(super) async fn disjoint_pair(
        &self,
        checker: &DisjointnessChecker<'_, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.endpoint
            .local_call(|| self.endpoint.admit_work(2))
            .await;
        if matches!(left, Type::RecursiveVar(_)) || matches!(right, Type::RecursiveVar(_)) {
            return self.unavailable(RelationSourceOperation::Operands).await;
        }
        Ok(self
            .endpoint
            .child_call(|| {
                checker.check_type_pair_with(RelationFieldReads::new(self.db), left, right, self)
            })
            .await)
    }

    async fn admit_disjoint_comparison(&self, left: Type<'db>, right: Type<'db>) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                let work = left
                    .inline_payload_bytes()
                    .checked_add(right.inline_payload_bytes())
                    .and_then(|work| work.checked_add(2))
                    .ok_or(RunError::Contract(
                        "disjointness comparison quotation overflow",
                    ))?;
                self.endpoint.admit_work(work)
            })
            .await;
        Ok(())
    }
}

macro_rules! unavailable_disjointness_effects {
    ($(fn $name:ident($($parameter:ident: $argument:ty),* $(,)?) -> $result:ty => $operation:ident;)*) => {
        $(
            async fn $name(&self, $($parameter: $argument),*) -> RunResult<$result> {
                $(let _ = $parameter;)*
                self.unavailable(RelationSourceOperation::$operation).await
            }
        )*
    };
}

impl<'run, 'db: 'run, 'a, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    DisjointnessEffects<'a, 'c, 'db> for BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;

    async fn check_type_pair(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.children
            .disjoint_pair(self.db, self.effects, checker, left, right)
            .await
    }

    async fn disjointness_intersections(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_intersection: IntersectionType<'db>,
        right_intersection: IntersectionType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_disjoint_intersection_with(
            left,
            right,
            DisjointIntersectionOperands::Both {
                left: left_intersection,
                right: right_intersection,
            },
            &BorrowedDisjointIntersection::new(self, checker),
        )
        .await
    }

    async fn disjointness_left_intersection(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_disjoint_intersection_with(
            left,
            right,
            DisjointIntersectionOperands::Left {
                intersection,
                other,
            },
            &BorrowedDisjointIntersection::new(self, checker),
        )
        .await
    }

    async fn disjointness_right_intersection(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_disjoint_intersection_with(
            left,
            right,
            DisjointIntersectionOperands::Right {
                intersection,
                other,
            },
            &BorrowedDisjointIntersection::new(self, checker),
        )
        .await
    }

    async fn type_is_always_falsy(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let truthiness = self.effects.type_truthiness(checker.env, ty).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(truthiness.is_always_false())
            })
            .await)
    }

    async fn type_is_always_truthy(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let truthiness = self.effects.type_truthiness(checker.env, ty).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(truthiness.is_always_true())
            })
            .await)
    }

    async fn disjointness_boolean(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        value: bool,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(ConstraintSet::from_bool(checker.constraints, value))
            })
            .await)
    }

    async fn check_type_pair_impl(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .child_call(|| {
                checker.check_type_pair_impl_with(
                    RelationFieldReads::new(self.db),
                    left,
                    right,
                    self,
                )
            })
            .await)
    }

    async fn disjointness_has_context(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
    ) -> RunResult<bool> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(checker.report_context().is_some())
            })
            .await)
    }

    async fn disjointness_clear_context(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
    ) -> RunResult<()> {
        if DisjointnessEffects::disjointness_has_context(self, checker).await? {
            return self
                .unavailable(RelationSourceOperation::DisjointContext)
                .await;
        }
        Ok(())
    }

    async fn is_always_satisfied(
        &self,
        _checker: &DisjointnessChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.satisfy(constraints, true).await
    }

    async fn disjointness_literal_kinds_differ(
        &self,
        _checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: LiteralValueType<'db>,
        right: LiteralValueType<'db>,
    ) -> RunResult<bool> {
        self.admit_disjoint_comparison(Type::LiteralValue(left), Type::LiteralValue(right))
            .await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.check_completion()?;
                Ok(left.kind() != right.kind())
            })
            .await)
    }

    async fn disjointness_types_differ(
        &self,
        _checker: &DisjointnessChecker<'a, 'c, 'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        self.admit_disjoint_comparison(left, right).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.check_completion()?;
                Ok(left != right)
            })
            .await)
    }

    async fn disjointness_alias_origin(
        &self,
        _checker: &DisjointnessChecker<'a, 'c, 'db>,
        alias: GenericAlias<'db>,
    ) -> RunResult<StaticClassLiteral<'db>> {
        let fields = alias.field_requests(self.endpoint.field_request_context());
        Ok(self
            .endpoint
            .read_field(fields.origin(), &BorrowOrCopy)
            .await)
    }

    async fn or<F>(
        &self,
        checker: &DisjointnessChecker<'a, 'c, 'db>,
        value: ConstraintSet<'db, 'c>,
        next: impl FnOnce() -> F,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        F: Future<Output = RunResult<ConstraintSet<'db, 'c>>>,
    {
        if self.satisfy(value, true).await? {
            return Ok(value);
        }
        let next = next().await?;
        <Self as PairEffects<'a, 'c, 'db>>::combine_constraints(
            self,
            checker.constraints,
            ConstraintFoldKind::Any,
            value,
            next,
        )
        .await
    }

    unavailable_disjointness_effects! {
        fn disjointness_left_alias(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            alias: TypeAliasType<'db>,
        ) -> ConstraintSet<'db, 'c> => AliasValue;
        fn disjointness_right_alias(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            alias: TypeAliasType<'db>,
        ) -> ConstraintSet<'db, 'c> => AliasValue;
        fn disjointness_left_enum_complement(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            complement: EnumComplementType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => EnumRemainingLiterals;
        fn disjointness_right_enum_complement(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
            complement: EnumComplementType<'db>,
        ) -> ConstraintSet<'db, 'c> => EnumRemainingLiterals;
        fn disjointness_subclass_typeform(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            subclass_of: SubclassOfType<'db>,
            typeform: TypeFormType<'db>,
        ) -> ConstraintSet<'db, 'c> => SubclassInstance;
        fn disjointness_typevar_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            type_var: BoundTypeVarInstance<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => RecursivePair;
        fn disjointness_typevar_instance(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            type_var: BoundTypeVarInstance<'db>,
            instance: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => RecursivePair;
        fn disjointness_typevar_bounds(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            tvar: BoundTypeVarInstance<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => TypevarBoundOrConstraints;
        fn disjointness_union(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            union: UnionType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => DisjointUnion;
        fn check_property_instance_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: PropertyInstanceType<'db>,
            right: PropertyInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Property;
        fn disjointness_interned_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: InternedType<'db>,
            right: InternedType<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownBoundMethod;
        fn disjointness_method_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: BoundMethodType<'db>,
            right: BoundMethodType<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundMethod;
        fn disjointness_wrappers(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            left_wrapper: MethodWrapper<'db>,
            right_wrapper: MethodWrapper<'db>,
        ) -> ConstraintSet<'db, 'c> => DisjointWrappers;
        fn disjointness_partials(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            left_partial: FunctoolsPartialInstance<'db>,
            right_partial: FunctoolsPartialInstance<'db>,
        ) -> ConstraintSet<'db, 'c> => SpecializePartialInstance;
        fn disjointness_protocols(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            left_proto: ProtocolInstanceType<'db>,
            right_proto: ProtocolInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn disjointness_protocol_special_form(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            special_form: SpecialFormType,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn disjointness_protocol_known_instance(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            known_instance: KnownInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn disjointness_protocol_members(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            ty: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn disjointness_protocol_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            nominal: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn disjointness_protocol_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            protocol: ProtocolInstanceType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn disjointness_alias_specializations(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left_alias: GenericAlias<'db>,
            right_alias: GenericAlias<'db>,
        ) -> ConstraintSet<'db, 'c> => ClassSpecialization;
        fn disjointness_class_alias(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            class: ClassLiteral<'db>,
            alias_b: GenericAlias<'db>,
        ) -> ConstraintSet<'db, 'c> => ClassDefaultSpecialization;
        fn disjointness_subclass_class(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            class_a: ClassType<'db>,
            class_b: ClassLiteral<'db>,
        ) -> ConstraintSet<'db, 'c> => Subclass;
        fn disjointness_subclass_alias(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            class_a: ClassType<'db>,
            alias_b: GenericAlias<'db>,
        ) -> ConstraintSet<'db, 'c> => Subclass;
        fn check_subclassof_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: SubclassOfType<'db>,
            right: SubclassOfType<'db>,
        ) -> ConstraintSet<'db, 'c> => Subclass;
        fn disjointness_subclass_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            subclass_of_ty: SubclassOfType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => Subclass;
        fn disjointness_special_form_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            special_form: SpecialFormType,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => SpecialFormInstanceFallback;
        fn disjointness_known_instance_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            known_instance: KnownInstanceType<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownInstanceFallback;
        fn disjointness_literal_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            literal: LiteralValueType<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => LiteralFallbackInstance;
        fn disjointness_bool_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => NominalInstance;
        fn disjointness_newtype_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            newtype: NewType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => NewtypeConcreteBase;
        fn disjointness_class_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            class: ClassLiteral<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => ClassLiteralMetaclassInstance;
        fn disjointness_alias_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            alias: GenericAlias<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => ClassMetaclassInstance;
        fn disjointness_function_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            function: FunctionType<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Function;
        fn disjointness_callable_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            class: KnownClass,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => Callable;
        fn disjointness_bound_method_fallback(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            a: BoundMethodType<'db>,
            b: BoundMethodType<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundMethod;
        fn disjointness_bound_method_functions(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            a: BoundMethodType<'db>,
            b: BoundMethodType<'db>,
            a_function: FunctionType<'db>,
            b_function: FunctionType<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundMethod;
        fn disjointness_bound_method_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundMethod;
        fn disjointness_known_method_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            method: KnownBoundMethodType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownBoundMethod;
        fn disjointness_descriptor_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownClassInstance;
        fn disjointness_callable_final_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            nominal: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => NominalInstance;
        fn disjointness_module_nominal(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => NominalInstance;
        fn disjointness_nominal_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            left_i: NominalInstanceType<'db>,
            right_i: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => NominalInstance;
        fn check_newtype_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: NewType<'db>,
            right: NewType<'db>,
        ) -> ConstraintSet<'db, 'c> => NewType;
        fn disjointness_property_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            property: PropertyInstanceType<'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => PropertyInstanceFallback;
        fn disjointness_slot_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownClassInstance;
        fn disjointness_bound_super_pair(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: BoundSuperType<'db>,
            right: BoundSuperType<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundSuper;
        fn disjointness_super_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownClassInstance;
        fn disjointness_typeddicts(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            left_td: TypedDictType<'db>,
            right_td: TypedDictType<'db>,
        ) -> ConstraintSet<'db, 'c> => TypedDict;
        fn disjointness_typeddict_other(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => TypedDictFallback;
        fn disjointness_left_recursive(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            left_recursive: RecursiveType<'db>,
        ) -> ConstraintSet<'db, 'c> => UnfoldRecursive;
        fn disjointness_right_recursive(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: Type<'db>,
            right: Type<'db>,
            right_recursive: RecursiveType<'db>,
        ) -> ConstraintSet<'db, 'c> => UnfoldRecursive;
        fn disjointness_transposed_typevar(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            subclass_of: SubclassOfType<'db>,
        ) -> Option<BoundTypeVarInstance<'db>> => TypeVarSubclass;
        fn disjointness_instance_approximation(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other: Type<'db>,
        ) -> Option<Type<'db>> => InstanceApproximation;
        fn disjointness_typevar_is_inferable(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left_tvar: BoundTypeVarInstance<'db>,
        ) -> bool => TypevarIsInferable;
        fn disjointness_same_typevar(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left_tvar: BoundTypeVarInstance<'db>,
            right_tvar: BoundTypeVarInstance<'db>,
        ) -> bool => SameTypevarOccurrence;
        fn disjointness_negative_contains_typevar(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            intersection: IntersectionType<'db>,
            tvar: BoundTypeVarInstance<'db>,
        ) -> bool => IntersectionNegativeContains;
        fn disjointness_same_wrapper_kind(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left_wrapper: MethodWrapper<'db>,
            right_wrapper: MethodWrapper<'db>,
        ) -> bool => DisjointWrappers;
        fn disjointness_nominal_is_final(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            nominal: NominalInstanceType<'db>,
        ) -> bool => NominalInstance;
        fn disjointness_callable_runtime_class(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            callable: CallableType<'db>,
        ) -> Option<KnownClass> => CallableRuntimeClass;
        fn disjointness_known_instance(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            other_class: KnownClass,
        ) -> Type<'db> => KnownClassInstance;
        fn disjointness_bound_function(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            a: BoundMethodType<'db>,
        ) -> Option<FunctionType<'db>> => BoundMethod;
        fn disjointness_function_names_differ(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            a_function: FunctionType<'db>,
            b_function: FunctionType<'db>,
        ) -> bool => Function;
        fn disjointness_same_sentinel(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left_sentinel: SentinelInstance<'db>,
            right_sentinel: SentinelInstance<'db>,
        ) -> bool => SameSentinel;
        fn disjointness_enum_class(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            left: EnumLiteralType<'db>,
        ) -> EnumClassLiteral<'db> => EnumInstanceLiteral;
        fn disjointness_enum_aliases_known(
            checker: &DisjointnessChecker<'a, 'c, 'db>,
            class: EnumClassLiteral<'db>,
        ) -> bool => EnumInstanceLiteral;
    }
}
