use itertools::Either;

use crate::{
    Db, DisplaySettings,
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, CallableType, ClassType, GenericContext,
        InferenceFlags, InvalidTypeExpressionError, KnownClass, StringLiteralType, Type,
        TypeAliasType, TypeContext, TypeMapping, TypeVarVariance, UnionBuilder,
        class::NamedTupleSpec,
        constraints::OwnedConstraintSet,
        generics::{Specialization, walk_generic_context},
        newtype::NewType,
        typevar::TypeVarInstance,
        variance::VarianceInferable,
        visitor,
    },
};
use ty_python_core::{definition::Definition, scope::ScopeId};

/// A Salsa-interned constraint set. This is only needed to have something appropriately small to
/// put in a [`KnownInstance::ConstraintSet`]. We don't actually manipulate these as part of using
/// constraint sets to check things like assignability; they're only used as a debugging aid in
/// mdtests. In theory, that means there's no need for this to be interned; being tracked would be
/// sufficient. However, we currently think that tracked structs are unsound w.r.t. salsa cycles,
/// so out of an abundance of caution, we are interning the struct.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct InternedConstraintSet<'db> {
    #[returns(ref)]
    pub(super) constraints: OwnedConstraintSet<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for InternedConstraintSet<'_> {}

/// A salsa-interned payload for `functools.partial(...)` instances.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct FunctoolsPartialInstance<'db> {
    pub wrapped: InternedType<'db>,
    pub partial: CallableType<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FunctoolsPartialInstance<'_> {}

/// Singleton types that are heavily special-cased by ty. Despite its name,
/// quite a different type to [`super::NominalInstanceType`].
///
/// In many ways, this enum behaves similarly to [`super::SpecialFormType`].
/// Unlike instances of that variant, however, `Type::KnownInstance`s do not exist
/// at a location that can be known prior to any analysis by ty, and each variant
/// of `KnownInstanceType` can have multiple instances (as, unlike `SpecialFormType`,
/// `KnownInstanceType` variants can hold associated data). Instances of this type
/// are generally created by operations at runtime in some way, such as a type alias
/// statement, a typevar definition, or an instance of `Generic[T]` in a class's
/// bases list.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub enum KnownInstanceType<'db> {
    /// The type of `Protocol[T]`, `Protocol[U, S]`, etc -- usually only found in a class's bases list.
    ///
    /// Note that unsubscripted `Protocol` is represented by [`super::SpecialFormType::Protocol`], not this type.
    SubscriptedProtocol(GenericContext<'db>),

    /// The type of `Generic[T]`, `Generic[U, S]`, etc -- usually only found in a class's bases list.
    ///
    /// Note that unsubscripted `Generic` is represented by [`super::SpecialFormType::Generic`], not this type.
    SubscriptedGeneric(GenericContext<'db>),

    /// A single instance of `typing.TypeVar`
    TypeVar(TypeVarInstance<'db>),

    /// A single instance of `typing.TypeAliasType` (PEP 695 type alias)
    TypeAliasType(TypeAliasType<'db>),

    /// A single instance of `warnings.deprecated` or `typing_extensions.deprecated`
    Deprecated(DeprecatedInstance<'db>),

    /// A single instance of `dataclasses.Field`
    Field(FieldInstance<'db>),

    /// A constraint set, which is exposed in mdtests as an instance of
    /// `ty_extensions.ConstraintSet`.
    ConstraintSet(InternedConstraintSet<'db>),

    /// A generic context, which is exposed in mdtests as an instance of
    /// `ty_extensions.GenericContext`.
    GenericContext(GenericContext<'db>),

    /// A specialization, which is exposed in mdtests as an instance of
    /// `ty_extensions.Specialization`.
    Specialization(Specialization<'db>),

    /// A single instance of `types.UnionType`, which stores the elements of
    /// a PEP 604 union, or a `typing.Union`.
    UnionType(UnionTypeInstance<'db>),

    /// A single instance of `typing.Literal`
    Literal(InternedType<'db>),

    /// A single instance of `typing.Annotated`
    Annotated(InternedType<'db>),

    /// An instance of `typing.GenericAlias` representing a `type[...]` expression.
    TypeGenericAlias(InternedType<'db>),

    /// An instance of `typing.GenericAlias` representing a `Callable[...]` expression.
    Callable(CallableType<'db>),

    /// A literal string which is the right-hand side of a PEP 613 `TypeAlias`.
    LiteralStringAlias(InternedType<'db>),

    /// An identity callable created with `typing.NewType(name, base)`, which behaves like a
    /// subtype of `base` in type expressions. See the `struct NewType` payload for an example.
    NewType(NewType<'db>),

    /// The inferred spec for a functional `NamedTuple` class.
    NamedTupleSpec(NamedTupleSpec<'db>),

    /// A `functools.partial(func, ...)` call result where we could determine
    /// the remaining callable signature after binding some arguments.
    FunctoolsPartial(FunctoolsPartialInstance<'db>),
}

pub(super) fn walk_known_instance_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    known_instance: KnownInstanceType<'db>,
    visitor: &V,
) {
    match known_instance {
        KnownInstanceType::SubscriptedProtocol(context)
        | KnownInstanceType::SubscriptedGeneric(context) => {
            walk_generic_context(db, context, visitor);
        }
        KnownInstanceType::TypeVar(typevar) => {
            visitor.visit_type_var_type(db, typevar);
        }
        KnownInstanceType::TypeAliasType(type_alias) => {
            visitor.visit_type_alias_type(db, type_alias);
        }
        KnownInstanceType::Deprecated(_)
        | KnownInstanceType::ConstraintSet(_)
        | KnownInstanceType::GenericContext(_)
        | KnownInstanceType::Specialization(_) => {
            // Nothing to visit
        }
        KnownInstanceType::Field(field) => {
            if let Some(default_ty) = field.default_type(db) {
                visitor.visit_type(db, default_ty);
            }
            if let Some((input_ty, output_ty)) = field.converter(db) {
                visitor.visit_type(db, input_ty);
                visitor.visit_type(db, output_ty);
            }
        }
        KnownInstanceType::UnionType(instance) => {
            if let Ok(union_type) = instance.union_type(db) {
                visitor.visit_type(db, *union_type);
            }
        }
        KnownInstanceType::Literal(ty)
        | KnownInstanceType::Annotated(ty)
        | KnownInstanceType::TypeGenericAlias(ty)
        | KnownInstanceType::LiteralStringAlias(ty) => {
            visitor.visit_type(db, ty.inner(db));
        }
        KnownInstanceType::Callable(callable) => {
            visitor.visit_callable_type(db, callable);
        }
        KnownInstanceType::NewType(newtype) => {
            visitor.visit_type(db, newtype.concrete_base_type(db));
        }
        KnownInstanceType::NamedTupleSpec(spec) => {
            for field in spec.fields(db) {
                visitor.visit_type(db, field.ty);
            }
        }
        KnownInstanceType::FunctoolsPartial(partial) => {
            visitor.visit_callable_type(db, partial.partial(db));
        }
    }
}

impl<'db> VarianceInferable<'db> for KnownInstanceType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        match self {
            KnownInstanceType::TypeAliasType(type_alias) => {
                type_alias.raw_value_type(db).variance_of(db, typevar)
            }
            _ => TypeVarVariance::Bivariant,
        }
    }
}

impl<'db> KnownInstanceType<'db> {
    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            // Nothing to normalize
            Self::SubscriptedProtocol(context) => Some(Self::SubscriptedProtocol(context)),
            Self::SubscriptedGeneric(context) => Some(Self::SubscriptedGeneric(context)),
            Self::Deprecated(deprecated) => Some(Self::Deprecated(deprecated)),
            Self::ConstraintSet(set) => Some(Self::ConstraintSet(set)),
            Self::TypeVar(typevar) => Some(Self::TypeVar(typevar)),
            Self::TypeAliasType(type_alias) => Some(Self::TypeAliasType(type_alias)),
            Self::Field(field) => field
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::Field),
            Self::UnionType(union_type) => union_type
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::UnionType),
            Self::Literal(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::Literal),
            Self::Annotated(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::Annotated),
            Self::TypeGenericAlias(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::TypeGenericAlias),
            Self::LiteralStringAlias(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::LiteralStringAlias),
            Self::Callable(callable) => callable
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::Callable),
            Self::NewType(newtype) => newtype
                .try_map_base_class_type(db, |class_type| {
                    class_type.recursive_type_normalized_impl(db, div, true)
                })
                .map(Self::NewType),
            Self::GenericContext(generic) => Some(Self::GenericContext(generic)),
            Self::Specialization(specialization) => specialization
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::Specialization),
            Self::NamedTupleSpec(spec) => spec
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::NamedTupleSpec),
            Self::FunctoolsPartial(partial) => partial
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::FunctoolsPartial),
        }
    }

    pub(super) fn class(self, db: &'db dyn Db) -> KnownClass {
        match self {
            Self::SubscriptedProtocol(_) | Self::SubscriptedGeneric(_) => KnownClass::SpecialForm,
            Self::TypeVar(typevar_instance) if typevar_instance.is_paramspec(db) => {
                KnownClass::ParamSpec
            }
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(TypeAliasType::PEP695(alias)) if alias.is_specialized(db) => {
                KnownClass::GenericAlias
            }
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
            Self::Deprecated(_) => KnownClass::Deprecated,
            Self::Field(_) => KnownClass::Field,
            Self::ConstraintSet(_) => KnownClass::ConstraintSet,
            Self::GenericContext(_) => KnownClass::GenericContext,
            Self::Specialization(_) => KnownClass::Specialization,
            Self::UnionType(_) => KnownClass::UnionType,
            Self::Literal(_)
            | Self::Annotated(_)
            | Self::TypeGenericAlias(_)
            | Self::Callable(_) => KnownClass::GenericAlias,
            Self::LiteralStringAlias(_) => KnownClass::Str,
            Self::NewType(_) => KnownClass::NewType,
            Self::NamedTupleSpec(_) => KnownClass::Sequence,
            Self::FunctoolsPartial(_) => KnownClass::FunctoolsPartial,
        }
    }

    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        self.class(db).to_class_literal(db)
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, an alias created using the `type` statement is an instance of
    /// `typing.TypeAliasType`, so `KnownInstanceType::TypeAliasType(_).instance_fallback(db)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing.TypeAliasType> })`.
    pub(super) fn instance_fallback(self, db: &'db dyn Db) -> Type<'db> {
        self.class(db).to_instance(db)
    }

    /// Return `true` if this symbol is an instance of `class`.
    pub(super) fn is_instance_of(self, db: &dyn Db, class: ClassType) -> bool {
        self.class(db).is_subclass_of(db, class)
    }

    /// Return the repr of the symbol at runtime
    pub(super) fn repr(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        self.display_with(db, DisplaySettings::default())
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        match self {
            KnownInstanceType::TypeVar(typevar) => match type_mapping {
                TypeMapping::BindLegacyTypevars(binding_context) => Type::TypeVar(
                    BoundTypeVarInstance::new(db, typevar, *binding_context, None),
                ),
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::Promote(..)
                | TypeMapping::BindSelf(..)
                | TypeMapping::ReplaceSelf { .. }
                | TypeMapping::Materialize(_)
                | TypeMapping::ReplaceParameterDefaults
                | TypeMapping::EagerExpansion
                | TypeMapping::RescopeReturnCallables(_) => Type::KnownInstance(self),
            },
            KnownInstanceType::UnionType(instance) => {
                Type::KnownInstance(KnownInstanceType::UnionType(
                    instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            KnownInstanceType::Annotated(ty) => {
                Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(
                    db,
                    ty.inner(db)
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                )))
            }
            KnownInstanceType::Callable(callable_type) => {
                Type::KnownInstance(KnownInstanceType::Callable(
                    callable_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            KnownInstanceType::FunctoolsPartial(partial) => {
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(
                    partial.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            KnownInstanceType::TypeGenericAlias(ty) => {
                Type::KnownInstance(KnownInstanceType::TypeGenericAlias(InternedType::new(
                    db,
                    ty.inner(db)
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                )))
            }

            KnownInstanceType::SubscriptedProtocol(_)
            | KnownInstanceType::SubscriptedGeneric(_)
            | KnownInstanceType::TypeAliasType(_)
            | KnownInstanceType::Deprecated(_)
            | KnownInstanceType::Field(_)
            | KnownInstanceType::ConstraintSet(_)
            | KnownInstanceType::GenericContext(_)
            | KnownInstanceType::Specialization(_)
            | KnownInstanceType::Literal(_)
            | KnownInstanceType::LiteralStringAlias(_)
            | KnownInstanceType::NamedTupleSpec(_)
            | KnownInstanceType::NewType(_) => {
                // TODO: For some of these, we may need to apply the type mapping to inner types.
                Type::KnownInstance(self)
            }
        }
    }
}

/// Data regarding a `warnings.deprecated` or `typing_extensions.deprecated` decorator.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct DeprecatedInstance<'db> {
    /// The message for the deprecation
    pub(crate) message: Option<StringLiteralType<'db>>,
}

/// Contains information about instances of `dataclasses.Field`, typically created using
/// `dataclasses.field()`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct FieldInstance<'db> {
    /// The type of the default value for this field. This is derived from the `default` or
    /// `default_factory` arguments to `dataclasses.field()`.
    pub default_type: Option<Type<'db>>,

    /// Whether this field is part of the `__init__` signature, or not.
    pub init: bool,

    /// Whether or not this field can only be passed as a keyword argument to `__init__`.
    pub kw_only: Option<bool>,

    /// This name is used to provide an alternative parameter name in the synthesized `__init__` method.
    pub alias: Option<Box<str>>,

    /// The converter types for this field, if a `converter` argument was provided.
    /// The first element is the input type (first positional parameter), the second is the
    /// output type (return type of the converter callable).
    pub converter: Option<(Type<'db>, Type<'db>)>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FieldInstance<'_> {}

impl<'db> FieldInstance<'db> {
    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let default_type = match self.default_type(db) {
            Some(default) if nested => Some(default.recursive_type_normalized_impl(db, div, true)?),
            Some(default) => Some(
                default
                    .recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let converter = match self.converter(db) {
            Some((input_ty, output_ty)) if nested => Some((
                input_ty.recursive_type_normalized_impl(db, div, true)?,
                output_ty.recursive_type_normalized_impl(db, div, true)?,
            )),
            Some((input_ty, output_ty)) => Some((
                input_ty
                    .recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
                output_ty
                    .recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            )),
            None => None,
        };
        Some(FieldInstance::new(
            db,
            default_type,
            self.init(db),
            self.kw_only(db),
            self.alias(db),
            converter,
        ))
    }
}

/// Contains information about a `types.UnionType` instance built from a PEP 604
/// union or a legacy `typing.Union[…]` annotation in a value expression context,
/// e.g. `IntOrStr = int | str` or `IntOrStr = Union[int, str]`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct UnionTypeInstance<'db> {
    /// You probably don't want to access this field outside `UnionTypeInstance`
    /// internals.
    ///
    /// This field is the types of the elements of this union, as they were inferred
    /// in a value expression context. For `int | str`, this would contain
    /// `<class 'int'>` and `<class 'str'>`. For `Union[int, str]`, this field is
    /// `None`, as we infer the elements as type expressions.
    ///
    /// Use `value_expression_types` to get the corresponding value expression types.
    #[returns(ref)]
    _value_expr_types: Option<[Type<'db>; 2]>,

    /// The type of the full union, which can be used when this `UnionType` instance
    /// is used in a type expression context. For `int | str`, this would contain
    /// `Ok(int | str)`. If any of the element types could not be converted, this
    /// contains the first encountered error.
    #[returns(ref)]
    pub(super) union_type: Result<Type<'db>, InvalidTypeExpressionError<'db>>,
}

impl get_size2::GetSize for UnionTypeInstance<'_> {}

impl<'db> UnionTypeInstance<'db> {
    pub(crate) fn from_value_expression_types(
        db: &'db dyn Db,
        value_expr_types: [Type<'db>; 2],
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Type<'db> {
        let mut builder = UnionBuilder::new(db);
        for ty in &value_expr_types {
            match ty.in_type_expression(db, scope_id, typevar_binding_context, inference_flags) {
                Ok(ty) => builder.add_in_place(ty),
                Err(error) => {
                    return Type::KnownInstance(KnownInstanceType::UnionType(
                        UnionTypeInstance::new(db, Some(value_expr_types), Err(error)),
                    ));
                }
            }
        }

        Type::KnownInstance(KnownInstanceType::UnionType(UnionTypeInstance::new(
            db,
            Some(value_expr_types),
            Ok(builder.build()),
        )))
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        if let Ok(union_type) = self.union_type(db) {
            UnionTypeInstance::new(
                db,
                self._value_expr_types(db),
                Ok(union_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
            )
        } else {
            self
        }
    }

    /// Get the types of the elements of this union as they would appear in a value
    /// expression context. For a PEP 604 union, we return the actual types that were
    /// inferred when we encountered the union in a value expression context. For a
    /// legacy `typing.Union[…]` annotation, we turn the type-expression types into
    /// their corresponding value-expression types, i.e. we turn instances like `int`
    /// into class literals like `<class 'int'>`. This operation is potentially lossy.
    pub(crate) fn value_expression_types(
        self,
        db: &'db dyn Db,
    ) -> Result<impl Iterator<Item = Type<'db>> + 'db, InvalidTypeExpressionError<'db>> {
        let to_class_literal = |ty: Type<'db>| {
            ty.as_nominal_instance()
                .and_then(|instance| {
                    instance
                        .class(db)
                        .static_class_literal(db)
                        .map(|(lit, _)| Type::ClassLiteral(lit.into()))
                })
                .unwrap_or_else(Type::unknown)
        };

        if let Some(value_expr_types) = self._value_expr_types(db) {
            Ok(Either::Left(value_expr_types.iter().copied()))
        } else {
            match self.union_type(db).clone()? {
                Type::Union(union) => Ok(Either::Right(Either::Left(
                    union.elements(db).iter().copied().map(to_class_literal),
                ))),
                ty => Ok(Either::Right(Either::Right(std::iter::once(
                    to_class_literal(ty),
                )))),
            }
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        // The `Divergent` elimination rules are different within union types.
        // See `UnionType::recursive_type_normalized_impl` for details.
        let value_expr_types = match self._value_expr_types(db).as_ref() {
            Some([first, second]) if nested => Some([
                first.recursive_type_normalized_impl(db, div, nested)?,
                second.recursive_type_normalized_impl(db, div, nested)?,
            ]),
            Some([first, second]) => Some([
                first
                    .recursive_type_normalized_impl(db, div, nested)
                    .unwrap_or(div),
                second
                    .recursive_type_normalized_impl(db, div, nested)
                    .unwrap_or(div),
            ]),
            None => None,
        };
        let union_type = match self.union_type(db).clone() {
            Ok(ty) if nested => Ok(ty.recursive_type_normalized_impl(db, div, nested)?),
            Ok(ty) => Ok(ty
                .recursive_type_normalized_impl(db, div, nested)
                .unwrap_or(div)),
            Err(err) => Err(err),
        };

        Some(Self::new(db, value_expr_types, union_type))
    }
}

impl<'db> FunctoolsPartialInstance<'db> {
    /// Normalizes both the wrapped callable and the exposed reduced callable recursively.
    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            InternedType::new(
                db,
                self.wrapped(db)
                    .inner(db)
                    .recursive_type_normalized_impl(db, div, nested)?,
            ),
            self.partial(db)
                .recursive_type_normalized_impl(db, div, nested)?,
        ))
    }

    /// Applies a type mapping to both the wrapped callable and the exposed reduced callable.
    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self::new(
            db,
            InternedType::new(
                db,
                self.wrapped(db)
                    .inner(db)
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
            self.partial(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
        )
    }
}

/// A salsa-interned `Type`
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct InternedType<'db> {
    pub(super) inner: Type<'db>,
}

impl get_size2::GetSize for InternedType<'_> {}

impl<'db> InternedType<'db> {
    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let inner = if nested {
            self.inner(db)
                .recursive_type_normalized_impl(db, div, nested)?
        } else {
            self.inner(db)
                .recursive_type_normalized_impl(db, div, nested)
                .unwrap_or(div)
        };
        Some(InternedType::new(db, inner))
    }
}
