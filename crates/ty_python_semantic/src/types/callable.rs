use ruff_python_ast::name::Name;
use smallvec::{SmallVec, smallvec_inline};

use crate::{
    Db, FxOrderSet,
    place::Place,
    semantic_index::definition::Definition,
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, CallableSignature, ClassType,
        FindLegacyTypeVarsVisitor, KnownInstanceType, LiteralValueTypeKind, MemberLookupPolicy,
        Parameter, Parameters, Signature, SubclassOfInner, Type, TypeContext, TypeMapping,
        TypeVarBoundOrConstraints, UnionType,
        constraints::{ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension},
        generics::InferableTypeVars,
        relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelation},
        visitor, walk_signature,
    },
};

impl<'db> Type<'db> {
    /// Create a callable type with a single non-overloaded signature.
    pub(crate) fn single_callable(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::single(db, signature))
    }

    /// Create a non-overloaded, function-like callable type with a single signature.
    ///
    /// A function-like callable will bind `self` when accessed as an attribute on an instance.
    pub(crate) fn function_like_callable(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::function_like(db, signature))
    }

    /// Create a non-overloaded callable type which represents the value bound to a `ParamSpec`
    /// type variable.
    pub(crate) fn paramspec_value_callable(
        db: &'db dyn Db,
        parameters: Parameters<'db>,
    ) -> Type<'db> {
        Type::Callable(CallableType::paramspec_value(db, parameters))
    }

    pub(crate) fn try_upcast_to_callable(self, db: &'db dyn Db) -> Option<CallableTypes<'db>> {
        match self {
            Type::Callable(callable) => Some(CallableTypes::one(callable)),

            Type::Dynamic(_) => Some(CallableTypes::one(CallableType::function_like(
                db,
                Signature::dynamic(self),
            ))),

            Type::FunctionLiteral(function_literal) => {
                Some(CallableTypes::one(function_literal.into_callable_type(db)))
            }
            Type::BoundMethod(bound_method) => {
                Some(CallableTypes::one(bound_method.into_callable_type(db)))
            }

            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                let call_symbol = self
                    .member_lookup_with_policy(
                        db,
                        Name::new_static("__call__"),
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place;

                if let Place::Defined(place) = call_symbol
                    && place.is_definitely_defined()
                {
                    place.ty.try_upcast_to_callable(db)
                } else {
                    None
                }
            }
            Type::ClassLiteral(class_literal) => {
                Some(class_literal.identity_specialization(db).into_callable(db))
            }

            Type::GenericAlias(alias) => Some(ClassType::Generic(alias).into_callable(db)),

            Type::NewTypeInstance(newtype) => {
                newtype.concrete_base_type(db).try_upcast_to_callable(db)
            }

            // TODO: This is unsound so in future we can consider an opt-in option to disable it.
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(class) => Some(class.into_callable(db)),
                SubclassOfInner::TypeVar(tvar) => match tvar.typevar(db).bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        let upcast_callables = bound.to_meta_type(db).try_upcast_to_callable(db)?;
                        Some(upcast_callables.map(|callable| {
                            let signatures = callable
                                .signatures(db)
                                .into_iter()
                                .map(|sig| sig.clone().with_return_type(Type::TypeVar(tvar)));
                            CallableType::new(
                                db,
                                CallableSignature::from_overloads(signatures),
                                callable.kind(db),
                            )
                        }))
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        let mut callables = SmallVec::new();
                        for constraint in constraints.elements(db) {
                            let element_upcast =
                                constraint.to_meta_type(db).try_upcast_to_callable(db)?;
                            for callable in element_upcast.into_inner() {
                                let signatures = callable
                                    .signatures(db)
                                    .into_iter()
                                    .map(|sig| sig.clone().with_return_type(Type::TypeVar(tvar)));
                                callables.push(CallableType::new(
                                    db,
                                    CallableSignature::from_overloads(signatures),
                                    callable.kind(db),
                                ));
                            }
                        }
                        Some(CallableTypes::new(callables))
                    }
                    None => Some(CallableTypes::one(CallableType::single(
                        db,
                        Signature::new(Parameters::gradual_form(), Type::TypeVar(tvar)),
                    ))),
                },
                SubclassOfInner::Dynamic(_) => Some(CallableTypes::one(CallableType::single(
                    db,
                    Signature::new(Parameters::unknown(), Type::from(subclass_of_ty)),
                ))),
            },

            Type::Union(union) => {
                let mut callables = SmallVec::new();
                for element in union.elements(db) {
                    let element_callable = element.try_upcast_to_callable(db)?;
                    callables.extend(element_callable.into_inner());
                }
                Some(CallableTypes::new(callables))
            }

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(enum_literal) => enum_literal
                    .enum_class_instance(db)
                    .try_upcast_to_callable(db),
                _ => None,
            },

            Type::TypeAlias(alias) => alias.value_type(db).try_upcast_to_callable(db),

            Type::KnownBoundMethod(method) => Some(CallableTypes::one(CallableType::new(
                db,
                CallableSignature::from_overloads(method.signatures(db)),
                CallableTypeKind::Regular,
            ))),

            Type::WrapperDescriptor(wrapper_descriptor) => {
                Some(CallableTypes::one(CallableType::new(
                    db,
                    CallableSignature::from_overloads(wrapper_descriptor.signatures(db)),
                    CallableTypeKind::Regular,
                )))
            }

            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => {
                Some(CallableTypes::one(CallableType::single(
                    db,
                    Signature::new(
                        Parameters::new(
                            db,
                            [Parameter::positional_only(None)
                                .with_annotated_type(newtype.base(db).instance_type(db))],
                        ),
                        Type::NewTypeInstance(newtype),
                    ),
                )))
            }

            Type::Never
            | Type::DataclassTransformer(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_) => None,

            // TODO
            Type::DataclassDecorator(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::Intersection(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_) => None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum CallableTypeKind {
    /// Represents regular callable objects.
    Regular,

    /// Represents function-like objects, like the synthesized methods of dataclasses or
    /// `NamedTuples`. These callables act like real functions when accessed as attributes on
    /// instances, i.e. they bind `self`.
    FunctionLike,

    /// A callable type that represents a staticmethod. These callables do not bind `self`
    /// when accessed as attributes on instances - they return the underlying function as-is.
    StaticMethodLike,

    /// A callable type that we believe represents a classmethod (i.e. it will unconditionally bind
    /// the first argument on `__get__`).
    ClassMethodLike,

    /// Represents the value bound to a `typing.ParamSpec` type variable.
    ParamSpecValue,
}

/// This type represents the set of all callable objects with a certain, possibly overloaded,
/// signature.
///
/// It can be written in type expressions using `typing.Callable`. `lambda` expressions are
/// inferred directly as `CallableType`s; all function-literal types are subtypes of a
/// `CallableType`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct CallableType<'db> {
    #[returns(ref)]
    pub(crate) signatures: CallableSignature<'db>,

    pub(super) kind: CallableTypeKind,
}

pub(super) fn walk_callable_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    ty: CallableType<'db>,
    visitor: &V,
) {
    for signature in &ty.signatures(db).overloads {
        walk_signature(db, signature, visitor);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CallableType<'_> {}

impl<'db> CallableType<'db> {
    pub(crate) fn single(db: &'db dyn Db, signature: Signature<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(signature),
            CallableTypeKind::Regular,
        )
    }

    pub(crate) fn function_like(db: &'db dyn Db, signature: Signature<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(signature),
            CallableTypeKind::FunctionLike,
        )
    }

    pub(crate) fn paramspec_value(
        db: &'db dyn Db,
        parameters: Parameters<'db>,
    ) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(Signature::new(parameters, Type::unknown())),
            CallableTypeKind::ParamSpecValue,
        )
    }

    /// Create a callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> CallableType<'db> {
        Self::single(db, Signature::unknown())
    }

    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::FunctionLike)
    }

    pub(crate) fn is_classmethod_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::ClassMethodLike)
    }

    pub(crate) fn is_staticmethod_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::StaticMethodLike)
    }

    pub(crate) fn bind_self(
        self,
        db: &'db dyn Db,
        self_type: Option<Type<'db>>,
    ) -> CallableType<'db> {
        CallableType::new(
            db,
            self.signatures(db).bind_self(db, self_type),
            self.kind(db),
        )
    }

    pub(crate) fn apply_self(self, db: &'db dyn Db, self_type: Type<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            self.signatures(db).apply_self(db, self_type),
            self.kind(db),
        )
    }

    /// Create a callable type which represents a fully-static "bottom" callable.
    ///
    /// Specifically, this represents a callable type with a single signature:
    /// `(*args: object, **kwargs: object) -> Never`.
    pub(crate) fn bottom(db: &'db dyn Db) -> CallableType<'db> {
        Self::new(db, CallableSignature::bottom(), CallableTypeKind::Regular)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(CallableType::new(
            db,
            self.signatures(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.kind(db),
        ))
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        if let TypeMapping::RescopeReturnCallables(replacements) = type_mapping {
            return replacements.get(&self).copied().unwrap_or(self);
        }

        CallableType::new(
            db,
            self.signatures(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            self.kind(db),
        )
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.signatures(db)
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
    }

    /// Check whether this callable type has the given relation to another callable type.
    ///
    /// See [`Type::is_subtype_of`] and [`Type::is_assignable_to`] for more details.
    #[expect(clippy::too_many_arguments)]
    pub(super) fn has_relation_to_impl<'c>(
        self,
        db: &'db dyn Db,
        other: Self,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation,
        relation_visitor: &HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &IsDisjointVisitor<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        if other.is_function_like(db) && !self.is_function_like(db) {
            return ConstraintSet::from_bool(constraints, false);
        }

        self.signatures(db).has_relation_to_impl(
            db,
            other.signatures(db),
            constraints,
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        )
    }
}

/// Converting a type "into a callable" can possibly return a _union_ of callables. Eventually,
/// when coercing that result to a single type, you'll get a `UnionType`. But this lets you handle
/// that result as a list of `CallableType`s before merging them into a `UnionType` should that be
/// helpful.
///
/// Note that this type is guaranteed to contain at least one callable. If you need to support "no
/// callables" as a possibility, use `Option<CallableTypes>`.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct CallableTypes<'db>(SmallVec<[CallableType<'db>; 1]>);

impl<'db> CallableTypes<'db> {
    pub(super) fn new(callables: SmallVec<[CallableType<'db>; 1]>) -> Self {
        assert!(!callables.is_empty(), "CallableTypes should not be empty");
        CallableTypes(callables)
    }

    pub(crate) fn one(callable: CallableType<'db>) -> Self {
        CallableTypes(smallvec_inline![callable])
    }

    pub(crate) fn from_elements(callables: impl IntoIterator<Item = CallableType<'db>>) -> Self {
        let callables: SmallVec<_> = callables.into_iter().collect();
        assert!(!callables.is_empty(), "CallableTypes should not be empty");
        CallableTypes(callables)
    }

    pub(crate) fn exactly_one(self) -> Option<CallableType<'db>> {
        match self.0.as_slice() {
            [single] => Some(*single),
            _ => None,
        }
    }

    pub(super) fn as_slice(&self) -> &[CallableType<'db>] {
        &self.0
    }

    pub(super) fn into_inner(self) -> SmallVec<[CallableType<'db>; 1]> {
        self.0
    }

    pub(crate) fn into_type(self, db: &'db dyn Db) -> Type<'db> {
        match self.0.as_slice() {
            [] => unreachable!("CallableTypes should not be empty"),
            [single] => Type::Callable(*single),
            slice => UnionType::from_elements(db, slice.iter().copied().map(Type::Callable)),
        }
    }

    pub(crate) fn map(self, mut f: impl FnMut(CallableType<'db>) -> CallableType<'db>) -> Self {
        Self::from_elements(self.0.iter().map(|element| f(*element)))
    }

    #[expect(clippy::too_many_arguments)]
    pub(crate) fn has_relation_to_impl<'c>(
        self,
        db: &'db dyn Db,
        other: CallableType<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation,
        relation_visitor: &HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &IsDisjointVisitor<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        self.0.iter().when_all(db, constraints, |element| {
            element.has_relation_to_impl(
                db,
                other,
                constraints,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            )
        })
    }
}
