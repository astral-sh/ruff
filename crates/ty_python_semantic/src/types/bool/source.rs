//! Shared truthiness decisions, with semantic dependencies supplied by the caller.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::Truthiness;

use super::{BoolError, TryBoolVisitor};
use crate::types::call::Bindings;
use crate::types::call::CallErrorKind;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::enums::EnumComplementType;
use crate::types::known_instance::InternedConstraintSet;
use crate::types::literal::{BytesLiteralType, EnumLiteralType, StringLiteralType};
use crate::types::newtype::NewType;
use crate::types::typevar::{BoundTypeVarInstance, TypeVarConstraints};
use crate::types::{
    CallArguments, CallDunderError, CallableType, ClassLiteral, ClassType, IntersectionType,
    KnownClass, KnownInstanceType, LiteralValueTypeKind, NominalInstanceType,
    PropertyInstanceClass, PropertyInstanceType, RecursiveType, SubclassOfInner, Type,
    TypeAliasType, TypeContext, TypeVarBoundOrConstraints, TypedDictType, UnionType,
};
use crate::{Db, ProgramEnvironment};

/// Semantic dependencies reached while determining a type's truthiness.
#[cfg(feature = "experimental-analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TruthinessOperation {
    RecursiveTruthiness,
    UnboundRecursiveVariable,
    RecursiveUnfold,
    TypedDictRequiredFields,
    TypedDictOpenness,
    TypedDictPresentFields,
    ConstraintSet,
    Instance,
    MetaclassInstance,
    TypeVarTranspose,
    TypeVarBounds,
    ConstraintTypes,
    DunderCall,
    DunderReturn,
    KnownInstance,
    ReturnAssignability,
    TupleSpec,
    ClassFinality,
    EnumLiterals,
    EnumInstance,
    TypeAlias,
    NewType,
}

pub(in crate::types) type BoolResult<'db> = Result<Truthiness, BoolError<'db>>;

pub(in crate::types) fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(in crate::types) struct BoolFacts;

shared_semantic_family! {
    #[synchronous(SynchronousBoolEffects)]
    pub(in crate::types) trait BoolEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn recurse(&self, ty: Type<'db>, allow_short_circuit: bool) -> Result<BoolResult<'db>, Self::Error>;
        #[operation(source)]
        async fn unbound_recursive(&self) -> Result<BoolResult<'db>, Self::Error>;
        #[operation(local)]
        async fn callable_has_runtime_class(&self, callable: CallableType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn typed_dict_has_required_fields(&self, td: TypedDictType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn typed_dict_is_closed(&self, td: TypedDictType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn typed_dict_has_present_fields(&self, td: TypedDictType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn constraints_satisfied(&self, constraints: InternedConstraintSet<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn property_class(&self, property: PropertyInstanceType<'db>) -> Result<PropertyInstanceClass<'db>, Self::Error>;
        #[operation(source)]
        async fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn class_metaclass_instance(&self, class: ClassLiteral<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn metaclass_instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn transpose_typevar(&self, inner: SubclassOfInner<'db>) -> Result<SubclassOfInner<'db>, Self::Error>;
        #[operation(source)]
        async fn typevar_bounds(&self, typevar: BoundTypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn constraint_types(&self, constraints: TypeVarConstraints<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn known_class_truthiness(&self, instance: NominalInstanceType<'db>) -> Result<Option<Truthiness>, Self::Error>;
        #[operation(source)]
        async fn dunders(&self, ty: Type<'db>) -> Result<BoolResult<'db>, Self::Error>;
        #[operation(source)]
        async fn union(&self, union: UnionType<'db>, allow_short_circuit: bool) -> Result<BoolResult<'db>, Self::Error>;
        #[operation(local)]
        async fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &'db [Type<'db>], cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn intersection_alternatives(&self, intersection: IntersectionType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn enum_literals(&self, complement: EnumComplementType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn enum_instance(&self, literal: EnumLiteralType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn string_nonempty(&self, literal: StringLiteralType<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn bytes_nonempty(&self, literal: BytesLiteralType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn alias(&self, alias: TypeAliasType<'db>, allow_short_circuit: bool) -> Result<BoolResult<'db>, Self::Error>;
        #[operation(source)]
        async fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn call_dunder(&self, ty: Type<'db>, name: &'static str) -> Result<Result<Bindings<'db>, CallDunderError<'db>>, Self::Error>;
        #[operation(source)]
        async fn return_type(&self, bindings: &Bindings<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn assignable(&self, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn tuple_truthiness(&self, instance: NominalInstanceType<'db>) -> Result<Option<Truthiness>, Self::Error>;
        #[operation(source)]
        async fn instance_is_final(&self, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn len_truthiness(&self, ty: Type<'db>) -> Result<BoolResult<'db>, Self::Error>;
    }

    #[finite_capability]
    impl BoolFacts {
        fn truthiness(&self, value: bool) -> Truthiness { Truthiness::from(value) }
        fn literal_kind<'db>(&self, literal: crate::types::LiteralValueType<'db>) -> LiteralValueTypeKind<'db> { literal.kind() }
        fn int_nonzero(&self, value: crate::types::literal::IntLiteralType) -> bool { value.as_i64() != 0 }
        fn subclass_inner<'db>(&self, subclass: crate::types::SubclassOfType<'db>) -> SubclassOfInner<'db> { subclass.subclass_of() }
        fn return_truthiness(&self, ty: Type<'_>) -> Truthiness {
            match ty.as_literal_value_kind() {
                Some(LiteralValueTypeKind::Bool(value)) => Truthiness::from(value),
                Some(LiteralValueTypeKind::Int(value)) => Truthiness::from(value.as_i64() != 0),
                _ => Truthiness::Ambiguous,
            }
        }
        fn fallback(&self, error: &BoolError<'_>) -> Truthiness { error.fallback_truthiness() }
        fn not_callable(&self, error: &BoolError<'_>) -> bool { matches!(error, BoolError::NotCallable { .. }) }
        fn first_truthiness(&self, previous: Option<Truthiness>, current: Truthiness) -> Option<Truthiness> { Some(previous.unwrap_or(current)) }
        fn differs(&self, previous: Option<Truthiness>, current: Truthiness) -> bool { previous != Some(current) }
        fn accumulated(&self, truthiness: Option<Truthiness>) -> Truthiness { truthiness.unwrap_or(Truthiness::Ambiguous) }
    }

    #[synchronous(try_bool_sync)]
    #[capabilities(effects = BoolEffects, facts = BoolFacts)]
    #[passive_values(ClassType::from, Type::from, Type::TypeVar, Truthiness::AlwaysTrue, Truthiness::AlwaysFalse, Truthiness::Ambiguous)]
    pub(in crate::types) async fn try_bool_with<'db, E: BoolEffects<'db>>(
        ty: Type<'db>,
        allow_short_circuit: bool,
        facts: BoolFacts,
        effects: &E,
    ) -> Result<BoolResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let result = match ty {
            Type::RecursiveVar(_) => effects.unbound_recursive().await?,
            Type::Callable(callable) if effects.callable_has_runtime_class(callable).await? => Ok(Truthiness::AlwaysTrue),
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never | Type::Callable(_)
            | Type::TypeIs(_) | Type::TypeGuard(_) | Type::TypeForm(_) => Ok(Truthiness::Ambiguous),
            Type::Recursive(recursive) => match effects.unfold(recursive).await? {
                Some(unfolded) => effects.recurse(unfolded, allow_short_circuit).await?,
                None => Ok(Truthiness::Ambiguous),
            },
            Type::TypedDict(td) => {
                if effects.typed_dict_has_required_fields(td).await? {
                    Ok(Truthiness::AlwaysTrue)
                } else if effects.typed_dict_is_closed(td).await?
                    && !effects.typed_dict_has_present_fields(td).await?
                {
                    Ok(Truthiness::AlwaysFalse)
                } else {
                    Ok(Truthiness::Ambiguous)
                }
            }
            Type::KnownInstance(KnownInstanceType::ConstraintSet(constraints)) => {
                let satisfied = effects.constraints_satisfied(constraints).await?;
                Ok(facts.truthiness(satisfied))
            }
            Type::KnownInstance(KnownInstanceType::Range { is_non_empty }) => Ok(facts.truthiness(is_non_empty)),
            Type::PropertyInstance(property) => match effects.property_class(property).await? {
                PropertyInstanceClass::Subclass(class) => {
                    let instance = effects.instance(class).await?;
                    effects.recurse(instance, allow_short_circuit).await?
                }
                PropertyInstanceClass::Builtin | PropertyInstanceClass::Enum => Ok(Truthiness::AlwaysTrue),
            },
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_) | Type::SlotDescriptor(_) | Type::BoundSuper(_)
            | Type::KnownInstance(_) | Type::SpecialForm(_) | Type::AlwaysTruthy => Ok(Truthiness::AlwaysTrue),
            Type::AlwaysFalsy => Ok(Truthiness::AlwaysFalse),
            Type::ClassLiteral(class) => {
                let instance = effects.class_metaclass_instance(class).await?;
                effects.recurse(instance, allow_short_circuit).await?
            }
            Type::GenericAlias(alias) => {
                let instance = effects.metaclass_instance(ClassType::from(alias)).await?;
                effects.recurse(instance, allow_short_circuit).await?
            }
            Type::SubclassOf(subclass) => match effects.transpose_typevar(facts.subclass_inner(subclass)).await? {
                SubclassOfInner::Dynamic(_) | SubclassOfInner::Protocol(_) => Ok(Truthiness::Ambiguous),
                SubclassOfInner::Class(class) => effects.recurse(Type::from(class), allow_short_circuit).await?,
                SubclassOfInner::TypeVar(typevar) => effects.recurse(Type::TypeVar(typevar), allow_short_circuit).await?,
            },
            Type::TypeVar(typevar) => match effects.typevar_bounds(typevar).await? {
                None => Ok(Truthiness::Ambiguous),
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => effects.recurse(bound, allow_short_circuit).await?,
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    let bound = effects.constraint_types(constraints).await?;
                    effects.recurse(bound, allow_short_circuit).await?
                }
            },
            Type::NominalInstance(instance) => match effects.known_class_truthiness(instance).await? {
                Some(truthiness) => Ok(truthiness),
                None => effects.dunders(ty).await?,
            },
            Type::ProtocolInstance(_) => effects.dunders(ty).await?,
            Type::Union(union) => effects.union(union, allow_short_circuit).await?,
            Type::Intersection(intersection) => match effects.intersection_alternatives(intersection).await? {
                Some(alternatives) => effects.recurse(alternatives, allow_short_circuit).await?,
                None => Ok(Truthiness::Ambiguous),
            },
            Type::EnumComplement(complement) => {
                let literals = effects.enum_literals(complement).await?;
                effects.recurse(literals, allow_short_circuit).await?
            }
            Type::LiteralValue(literal) => match facts.literal_kind(literal) {
                LiteralValueTypeKind::LiteralString => Ok(Truthiness::Ambiguous),
                LiteralValueTypeKind::Enum(literal) => {
                    let instance = effects.enum_instance(literal).await?;
                    effects.recurse(instance, allow_short_circuit).await?
                }
                LiteralValueTypeKind::Int(value) => Ok(facts.truthiness(facts.int_nonzero(value))),
                LiteralValueTypeKind::Bool(value) => Ok(facts.truthiness(value)),
                LiteralValueTypeKind::String(value) => {
                    let nonempty = effects.string_nonempty(value).await?;
                    Ok(facts.truthiness(nonempty))
                }
                LiteralValueTypeKind::Bytes(value) => {
                    let nonempty = effects.bytes_nonempty(value).await?;
                    Ok(facts.truthiness(nonempty))
                }
            },
            Type::TypeAlias(alias) => effects.alias(alias, allow_short_circuit).await?,
            Type::NewTypeInstance(newtype) => {
                let base = effects.newtype_base(newtype).await?;
                effects.recurse(base, allow_short_circuit).await?
            }
        };
        Ok(result)
    }

    #[synchronous(try_union_sync)]
    #[capabilities(effects = BoolEffects, facts = BoolFacts)]
    #[passive_values(Err, Type::Union, Truthiness::Ambiguous, BoolError::NotCallable, BoolError::Union)]
    pub(in crate::types) async fn try_union_with<'db, E: BoolEffects<'db>>(
        union: UnionType<'db>,
        allow_short_circuit: bool,
        facts: BoolFacts,
        effects: &E,
    ) -> Result<BoolResult<'db>, E::Error> {
        #[passive_state]
        let mut truthiness = None;
        #[passive_state]
        let mut all_not_callable = true;
        #[passive_state]
        let mut has_errors = false;
        let elements = effects.union_elements(union).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(element) = effects.next_element(elements, &mut cursor).await? {
            let element_truthiness = match effects.recurse(element, allow_short_circuit).await? {
                Ok(truthiness) => truthiness,
                Err(error) => {
                    has_errors = true;
                    all_not_callable = all_not_callable && facts.not_callable(&error);
                    facts.fallback(&error)
                }
            };
            truthiness = facts.first_truthiness(truthiness, element_truthiness);
            if facts.differs(truthiness, element_truthiness) {
                truthiness = Some(Truthiness::Ambiguous);
                if allow_short_circuit {
                    return Ok(Ok(Truthiness::Ambiguous));
                }
            }
        }
        if has_errors {
            if all_not_callable {
                return Ok(Err(BoolError::NotCallable { not_boolable_type: Type::Union(union) }));
            }
            return Ok(Err(BoolError::Union { union, truthiness: facts.accumulated(truthiness) }));
        }
        Ok(Ok(facts.accumulated(truthiness)))
    }

    #[synchronous(try_dunders_sync)]
    #[capabilities(effects = BoolEffects, facts = BoolFacts)]
    #[passive_values(Err, KnownClass::Bool, Truthiness::Ambiguous, BoolError::IncorrectReturnType, BoolError::IncorrectArguments, BoolError::NotCallable, BoolError::Other)]
    pub(in crate::types) async fn try_dunders_with<'db, E: BoolEffects<'db>>(
        ty: Type<'db>,
        facts: BoolFacts,
        effects: &E,
    ) -> Result<BoolResult<'db>, E::Error> {
        match effects.call_dunder(ty, "__bool__").await? {
            Ok(outcome) => {
                let return_type = effects.return_type(&outcome).await?;
                let bool_type = effects.known_instance(KnownClass::Bool).await?;
                if !effects.assignable(return_type, bool_type).await? {
                    return Ok(Err(BoolError::IncorrectReturnType { return_type, not_boolable_type: ty }));
                }
                Ok(Ok(facts.return_truthiness(return_type)))
            }
            Err(CallDunderError::PossiblyUnbound { bindings: outcome, .. }) => {
                let return_type = effects.return_type(&outcome).await?;
                let bool_type = effects.known_instance(KnownClass::Bool).await?;
                if !effects.assignable(return_type, bool_type).await? {
                    let return_type = effects.return_type(&outcome).await?;
                    return Ok(Err(BoolError::IncorrectReturnType { return_type, not_boolable_type: ty }));
                }
                // Don't trust a possibly missing `__bool__` method.
                Ok(Ok(Truthiness::Ambiguous))
            }
            Err(CallDunderError::MethodNotAvailable) => {
                if let Type::NominalInstance(instance) = ty {
                    // TODO: diagnose tuple subclasses whose `__bool__` return type conflicts
                    // with their length; the tuple fallback is otherwise unsound.
                    if let Some(truthiness) = effects.tuple_truthiness(instance).await? {
                        return Ok(Ok(truthiness));
                    }
                    // A subclass can add `__bool__`, so only final classes can use `__len__`.
                    if effects.instance_is_final(instance).await? {
                        return effects.len_truthiness(ty).await;
                    }
                }
                Ok(Ok(Truthiness::Ambiguous))
            }
            Err(CallDunderError::CallError(CallErrorKind::BindingError, bindings, _)) => {
                let return_type = effects.return_type(&bindings).await?;
                Ok(Err(BoolError::IncorrectArguments { truthiness: facts.return_truthiness(return_type), not_boolable_type: ty }))
            }
            Err(CallDunderError::CallError(CallErrorKind::NotCallable, _, _)) => Ok(Err(BoolError::NotCallable { not_boolable_type: ty })),
            Err(CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _, _)) => Ok(Err(BoolError::Other { not_boolable_type: ty })),
        }
    }

    #[synchronous(try_len_sync)]
    #[capabilities(effects = BoolEffects, facts = BoolFacts)]
    #[passive_values(KnownClass::SupportsIndex, Truthiness::Ambiguous, Truthiness::AlwaysTrue)]
    pub(in crate::types) async fn try_len_with<'db, E: BoolEffects<'db>>(
        ty: Type<'db>,
        facts: BoolFacts,
        effects: &E,
    ) -> Result<BoolResult<'db>, E::Error> {
        match effects.call_dunder(ty, "__len__").await? {
            Ok(outcome) => {
                let return_type = effects.return_type(&outcome).await?;
                let index_type = effects.known_instance(KnownClass::SupportsIndex).await?;
                if effects.assignable(return_type, index_type).await? {
                    Ok(Ok(facts.return_truthiness(return_type)))
                } else {
                    // TODO: report invalid `__len__` return types, as for `__bool__`.
                    Ok(Ok(Truthiness::Ambiguous))
                }
            }
            // A final type with neither method is always truthy.
            Err(CallDunderError::MethodNotAvailable) => Ok(Ok(Truthiness::AlwaysTrue)),
            // TODO: report errors during `__len__` calls, as for `__bool__`.
            Err(_) => Ok(Ok(Truthiness::Ambiguous)),
        }
    }
}

pub(super) struct OrdinaryBoolEffects<'env, 'visitor, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) visitor: &'visitor TryBoolVisitor<'db>,
}

impl<'db> SynchronousBoolEffects<'db> for OrdinaryBoolEffects<'_, '_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn recurse(
        &self,
        ty: Type<'db>,
        allow_short_circuit: bool,
    ) -> Result<BoolResult<'db>, Self::Error> {
        try_bool_sync(ty, allow_short_circuit, BoolFacts, self)
    }
    fn unbound_recursive(&self) -> Result<BoolResult<'db>, Self::Error> {
        unreachable!("semantic operation on an unbound recursive variable")
    }
    fn callable_has_runtime_class(&self, callable: CallableType<'db>) -> Result<bool, Self::Error> {
        Ok(callable.runtime_class(self.db).is_some())
    }
    fn unfold(&self, recursive: RecursiveType<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(recursive.unfold(self.db, self.env).into_unfolded())
    }
    fn typed_dict_has_required_fields(&self, td: TypedDictType<'db>) -> Result<bool, Self::Error> {
        Ok(td
            .items(self.db)
            .values()
            .any(crate::types::typed_dict::TypedDictField::is_required))
    }
    fn typed_dict_is_closed(&self, td: TypedDictType<'db>) -> Result<bool, Self::Error> {
        Ok(td.openness(self.db).is_closed())
    }
    fn typed_dict_has_present_fields(&self, td: TypedDictType<'db>) -> Result<bool, Self::Error> {
        Ok(td
            .items(self.db)
            .values()
            .any(|field| field.may_be_present(self.db)))
    }
    fn constraints_satisfied(
        &self,
        constraints: InternedConstraintSet<'db>,
    ) -> Result<bool, Self::Error> {
        let builder = ConstraintSetBuilder::new();
        let constraints = builder.load(self.db, self.env, constraints.constraints(self.db));
        Ok(constraints.is_always_satisfied(self.db, self.env))
    }
    fn property_class(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> Result<PropertyInstanceClass<'db>, Self::Error> {
        Ok(property.instance_class(self.db))
    }
    fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(self.db, self.env, class))
    }
    fn metaclass_instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(class.metaclass_instance_type(self.db, self.env))
    }
    fn class_metaclass_instance(&self, class: ClassLiteral<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(class.metaclass_instance_type(self.db, self.env))
    }
    fn transpose_typevar(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> Result<SubclassOfInner<'db>, Self::Error> {
        Ok(inner.with_transposed_type_var(self.db, self.env))
    }
    fn typevar_bounds(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error> {
        Ok(typevar
            .typevar(self.db)
            .bound_or_constraints(self.db, self.env))
    }
    fn constraint_types(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(constraints.as_type(self.db, self.env))
    }
    fn known_class_truthiness(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<Truthiness>, Self::Error> {
        Ok(instance.known_class(self.db).and_then(KnownClass::bool))
    }
    fn dunders(&self, ty: Type<'db>) -> Result<BoolResult<'db>, Self::Error> {
        try_dunders_sync(ty, BoolFacts, self)
    }
    fn union(
        &self,
        union: UnionType<'db>,
        allow_short_circuit: bool,
    ) -> Result<BoolResult<'db>, Self::Error> {
        try_union_sync(union, allow_short_circuit, BoolFacts, self)
    }
    fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(union.elements(self.db))
    }
    fn next_element(
        &self,
        elements: &'db [Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let next = elements.get(*cursor).copied();
        *cursor += usize::from(next.is_some());
        Ok(next)
    }
    fn intersection_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(intersection.finite_alternative_union(self.db, self.env))
    }
    fn enum_literals(&self, complement: EnumComplementType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(complement.remaining_literal_union(self.db, self.env))
    }
    fn enum_instance(&self, literal: EnumLiteralType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(literal.enum_class_instance(self.db, self.env))
    }
    fn string_nonempty(&self, literal: StringLiteralType<'db>) -> Result<bool, Self::Error> {
        Ok(!literal.value(self.db).is_empty())
    }
    fn bytes_nonempty(&self, literal: BytesLiteralType<'db>) -> Result<bool, Self::Error> {
        Ok(!literal.value(self.db).is_empty())
    }
    fn alias(
        &self,
        alias: TypeAliasType<'db>,
        allow_short_circuit: bool,
    ) -> Result<BoolResult<'db>, Self::Error> {
        Ok(self.visitor.visit(self.db, Type::TypeAlias(alias), || {
            alias.value_type(self.db).try_bool_impl(
                self.db,
                self.env,
                allow_short_circuit,
                self.visitor,
            )
        }))
    }
    fn newtype_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(newtype.concrete_base_type(self.db))
    }
    fn call_dunder(
        &self,
        ty: Type<'db>,
        name: &'static str,
    ) -> Result<Result<Bindings<'db>, CallDunderError<'db>>, Self::Error> {
        Ok(ty.try_call_dunder(
            self.db,
            self.env,
            name,
            CallArguments::none(),
            TypeContext::default(),
        ))
    }
    fn return_type(&self, bindings: &Bindings<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(bindings.return_type(self.db, self.env))
    }
    fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, self.env))
    }
    fn assignable(&self, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error> {
        Ok(source.is_assignable_to(self.db, self.env, target))
    }
    fn tuple_truthiness(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<Truthiness>, Self::Error> {
        Ok(instance
            .tuple_spec(self.db, self.env)
            .map(|spec| spec.truthiness()))
    }
    fn instance_is_final(&self, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error> {
        Ok(instance.class(self.db, self.env).is_final(self.db))
    }
    fn len_truthiness(&self, ty: Type<'db>) -> Result<BoolResult<'db>, Self::Error> {
        try_len_sync(ty, BoolFacts, self)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::types::set_theoretic::RecursivelyDefined;

    struct ScriptedEffects<'db> {
        db: &'db dyn Db,
        outcomes: RefCell<VecDeque<Result<BoolResult<'db>, &'static str>>>,
        visited: RefCell<Vec<Type<'db>>>,
        events: RefCell<Vec<&'static str>>,
    }

    impl<'db> ScriptedEffects<'db> {
        fn new(
            db: &'db dyn Db,
            outcomes: impl IntoIterator<Item = Result<BoolResult<'db>, &'static str>>,
        ) -> Self {
            Self {
                db,
                outcomes: RefCell::new(outcomes.into_iter().collect()),
                visited: RefCell::default(),
                events: RefCell::default(),
            }
        }
    }

    macro_rules! refuse_methods {
        ($(fn $name:ident($($argument:ident: $ty:ty),*) -> $result:ty;)*) => {
            $(fn $name(&self, $($argument: $ty),*) -> Result<$result, Self::Error> {
                Err(stringify!($name))
            })*
        };
    }

    impl<'db> SynchronousBoolEffects<'db> for ScriptedEffects<'db> {
        type Error = &'static str;

        fn checkpoint(&self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn recurse(
            &self,
            ty: Type<'db>,
            _allow_short_circuit: bool,
        ) -> Result<BoolResult<'db>, Self::Error> {
            self.visited.borrow_mut().push(ty);
            self.outcomes
                .borrow_mut()
                .pop_front()
                .ok_or("unexpected recursive evaluation")?
        }
        fn union(
            &self,
            union: UnionType<'db>,
            allow_short_circuit: bool,
        ) -> Result<BoolResult<'db>, Self::Error> {
            try_union_sync(union, allow_short_circuit, BoolFacts, self)
        }
        fn union_elements(&self, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error> {
            Ok(union.elements(self.db))
        }
        fn next_element(
            &self,
            elements: &'db [Type<'db>],
            cursor: &mut usize,
        ) -> Result<Option<Type<'db>>, Self::Error> {
            let next = elements.get(*cursor).copied();
            *cursor += usize::from(next.is_some());
            Ok(next)
        }
        fn known_class_truthiness(
            &self,
            _instance: NominalInstanceType<'db>,
        ) -> Result<Option<Truthiness>, Self::Error> {
            self.events.borrow_mut().push("known class");
            Ok(None)
        }
        fn dunders(&self, ty: Type<'db>) -> Result<BoolResult<'db>, Self::Error> {
            try_dunders_sync(ty, BoolFacts, self)
        }
        fn call_dunder(
            &self,
            _ty: Type<'db>,
            name: &'static str,
        ) -> Result<Result<Bindings<'db>, CallDunderError<'db>>, Self::Error> {
            self.events.borrow_mut().push(name);
            Err("dunder unavailable")
        }

        refuse_methods! {
            fn unbound_recursive() -> BoolResult<'db>;
            fn callable_has_runtime_class(_callable: CallableType<'db>) -> bool;
            fn unfold(_recursive: RecursiveType<'db>) -> Option<Type<'db>>;
            fn typed_dict_has_required_fields(_td: TypedDictType<'db>) -> bool;
            fn typed_dict_is_closed(_td: TypedDictType<'db>) -> bool;
            fn typed_dict_has_present_fields(_td: TypedDictType<'db>) -> bool;
            fn constraints_satisfied(_constraints: InternedConstraintSet<'db>) -> bool;
            fn property_class(_property: PropertyInstanceType<'db>) -> PropertyInstanceClass<'db>;
            fn instance(_class: ClassType<'db>) -> Type<'db>;
            fn metaclass_instance(_class: ClassType<'db>) -> Type<'db>;
            fn class_metaclass_instance(_class: ClassLiteral<'db>) -> Type<'db>;
            fn transpose_typevar(_inner: SubclassOfInner<'db>) -> SubclassOfInner<'db>;
            fn typevar_bounds(_typevar: BoundTypeVarInstance<'db>) -> Option<TypeVarBoundOrConstraints<'db>>;
            fn constraint_types(_constraints: TypeVarConstraints<'db>) -> Type<'db>;
            fn intersection_alternatives(_intersection: IntersectionType<'db>) -> Option<Type<'db>>;
            fn enum_literals(_complement: EnumComplementType<'db>) -> Type<'db>;
            fn enum_instance(_literal: EnumLiteralType<'db>) -> Type<'db>;
            fn string_nonempty(_literal: StringLiteralType<'db>) -> bool;
            fn bytes_nonempty(_literal: BytesLiteralType<'db>) -> bool;
            fn alias(_alias: TypeAliasType<'db>, _allow_short_circuit: bool) -> BoolResult<'db>;
            fn newtype_base(_newtype: NewType<'db>) -> Type<'db>;
            fn return_type(_bindings: &Bindings<'db>) -> Type<'db>;
            fn known_instance(_class: KnownClass) -> Type<'db>;
            fn assignable(_source: Type<'db>, _target: Type<'db>) -> bool;
            fn tuple_truthiness(_instance: NominalInstanceType<'db>) -> Option<Truthiness>;
            fn instance_is_final(_instance: NominalInstanceType<'db>) -> bool;
            fn len_truthiness(_ty: Type<'db>) -> BoolResult<'db>;
        }
    }

    fn union(db: &dyn Db) -> UnionType<'_> {
        UnionType::new(
            db,
            &[
                Type::int_literal(0),
                Type::int_literal(1),
                Type::int_literal(2),
            ][..],
            RecursivelyDefined::No,
        )
    }

    #[test]
    fn union_short_circuit_skips_later_semantic_dependencies() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let union = union(&db);
        let effects = ScriptedEffects::new(
            &db,
            [
                Ok(Ok(Truthiness::AlwaysTrue)),
                Ok(Ok(Truthiness::AlwaysFalse)),
                Err("later dependency"),
            ],
        );
        assert_eq!(
            try_bool_sync(Type::Union(union), true, BoolFacts, &effects),
            Ok(Ok(Truthiness::Ambiguous))
        );
        assert_eq!(*effects.visited.borrow(), union.elements(&db)[..2]);

        let effects = ScriptedEffects::new(
            &db,
            [
                Ok(Ok(Truthiness::AlwaysTrue)),
                Ok(Ok(Truthiness::AlwaysFalse)),
                Err("later dependency"),
            ],
        );
        assert_eq!(
            try_bool_sync(Type::Union(union), false, BoolFacts, &effects),
            Err("later dependency")
        );
        assert_eq!(*effects.visited.borrow(), union.elements(&db));
        Ok(())
    }

    #[test]
    fn ambiguous_union_elements_do_not_hide_an_operational_failure() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let effects = ScriptedEffects::new(
            &db,
            [
                Ok(Ok(Truthiness::Ambiguous)),
                Ok(Ok(Truthiness::Ambiguous)),
                Err("interrupted"),
            ],
        );
        assert_eq!(
            try_bool_sync(Type::Union(union(&db)), true, BoolFacts, &effects),
            Err("interrupted")
        );
        assert_eq!(effects.visited.borrow().len(), 3);
        Ok(())
    }

    #[test]
    fn union_keeps_semantic_errors_separate_from_operational_errors() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let union = union(&db);
        let effects = ScriptedEffects::new(
            &db,
            [
                Ok(Err(BoolError::IncorrectArguments {
                    not_boolable_type: Type::int_literal(0),
                    truthiness: Truthiness::AlwaysTrue,
                })),
                Ok(Ok(Truthiness::AlwaysTrue)),
                Ok(Ok(Truthiness::AlwaysTrue)),
            ],
        );
        assert_eq!(
            try_bool_sync(Type::Union(union), false, BoolFacts, &effects),
            Ok(Err(BoolError::Union {
                union,
                truthiness: Truthiness::AlwaysTrue
            }))
        );
        Ok(())
    }

    #[test]
    fn dunder_refusal_is_not_ambiguous_truthiness() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let effects = ScriptedEffects::new(&db, []);
        assert_eq!(
            try_bool_sync(Type::object(), true, BoolFacts, &effects),
            Err("dunder unavailable")
        );
        assert_eq!(*effects.events.borrow(), ["known class", "__bool__"]);
        Ok(())
    }
}
