use crate::SemanticEnvironment;
use crate::types::class::CodeGeneratorKind;
use crate::types::generics::{ApplySpecialization, Specialization};
use crate::types::mro::MroIterator;
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingVisitor, ClassLiteral, ClassType, DivergentType, DynamicType, KnownClass,
    KnownInstanceType, MaterializationKind, SpecialFormType, StaticMroError, Type, TypeContext,
    TypeMapping, TypedDictModule, todo_type,
};
use crate::{Db, DisplaySettings};

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum: all types that would be invalid to have as a
/// class base are transformed into [`ClassBase::unknown()`]
///
/// Note that a non-specialized generic class _cannot_ be a class base. When we see a
/// non-specialized generic class in any type expression (including the list of base classes), we
/// automatically construct the default specialization for that class.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum ClassBase<'db> {
    /// The `Any` special form used directly as a base class.
    ///
    /// This is distinct from [`ClassBase::Dynamic`] because a base expression whose inferred type
    /// is `Any` does not give the class the same gradual assignability as an explicit `Any` base.
    Any,
    Dynamic(DynamicType<'db>),
    Divergent(DivergentType),
    Class(ClassType<'db>),
    /// Although `Protocol` is not a class in typeshed's stubs, it is at runtime,
    /// and can appear in the MRO of a class.
    Protocol,
    /// Bare `Generic` cannot be subclassed directly in user code,
    /// but nonetheless appears in the MRO of classes that inherit from `Generic[T]`,
    /// `Protocol[T]`, or bare `Protocol`.
    Generic,
    TypedDict(TypedDictModule),
}

impl<'db> ClassBase<'db> {
    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        env: &SemanticEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Dynamic(dynamic) => Some(Self::Dynamic(dynamic.recursive_type_normalized())),
            Self::Divergent(_) => Some(self),
            Self::Class(class) => Some(Self::Class(
                class.recursive_type_normalized_impl(env, div, nested)?,
            )),
            Self::Any | Self::Protocol | Self::Generic | Self::TypedDict(_) => Some(self),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            ClassBase::Any => "Any",
            ClassBase::Class(class) => class.name(db),
            ClassBase::Dynamic(DynamicType::Any) => "Any",
            ClassBase::Dynamic(
                DynamicType::Unknown
                | DynamicType::UnknownGeneric(_)
                | DynamicType::InvalidConcatenateUnknown
                | DynamicType::AmbiguousOverload,
            ) => "Unknown",
            ClassBase::Dynamic(DynamicType::UnspecializedTypeVar) => "UnspecializedTypeVar",
            ClassBase::Dynamic(DynamicType::Todo(_)) => "@Todo",
            ClassBase::Divergent(_) => "Divergent",
            ClassBase::Protocol => "Protocol",
            ClassBase::Generic => "Generic",
            ClassBase::TypedDict(_) => "TypedDict",
        }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    pub(super) fn object(env: &SemanticEnvironment<'db>) -> Self {
        Self::Class(ClassType::object(env))
    }

    pub(super) const fn is_typed_dict(self) -> bool {
        self.typed_dict_module().is_some()
    }

    pub(super) const fn typed_dict_module(self) -> Option<TypedDictModule> {
        match self {
            ClassBase::TypedDict(module) => Some(module),
            _ => None,
        }
    }

    /// Return the identity of this base for method-resolution-order construction.
    ///
    /// The `TypedDict` module affects member lookup, but both special forms represent the same
    /// pseudo-base when detecting duplicate or conflicting bases.
    pub(super) const fn mro_identity(self) -> Self {
        match self {
            Self::TypedDict(_) => Self::TypedDict(TypedDictModule::Typing),
            _ => self,
        }
    }

    /// Return whether this is an explicit `Any` base.
    pub(super) const fn is_explicit_any_base(self) -> bool {
        matches!(self, ClassBase::Any)
    }

    /// Convert an explicit base while preserving a direct use of the `Any` special form.
    pub(super) fn try_from_explicit_base(
        env: &SemanticEnvironment<'db>,
        ty: Type<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Option<Self> {
        if matches!(ty, Type::SpecialForm(SpecialFormType::Any)) {
            Some(Self::Any)
        } else {
            Self::try_from_type(env, ty, subclass)
        }
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    pub(super) fn try_from_type(
        env: &SemanticEnvironment<'db>,
        ty: Type<'db>,
        subclass: Option<ClassLiteral<'db>>,
    ) -> Option<Self> {
        let db = env.db();
        match ty {
            Type::Dynamic(dynamic) => Some(Self::Dynamic(dynamic)),
            Type::Divergent(divergent) => Some(Self::Divergent(divergent)),
            Type::ClassLiteral(literal) => Some(Self::Class(literal.default_specialization(env))),
            Type::GenericAlias(generic) => Some(Self::Class(ClassType::Generic(generic))),
            Type::NominalInstance(instance)
                if instance.has_known_class(db, KnownClass::GenericAlias) =>
            {
                Self::try_from_type(env, todo_type!("GenericAlias instance"), subclass)
            }
            Type::SubclassOf(subclass_of) => subclass_of
                .subclass_of()
                .into_dynamic()
                .map(ClassBase::Dynamic),
            Type::Intersection(inter) => {
                let valid_element = inter
                    .positive(db)
                    .iter()
                    .find_map(|elem| ClassBase::try_from_type(env, *elem, subclass))?;

                if ty.is_disjoint_from(env, KnownClass::Type.to_instance(env)) {
                    None
                } else {
                    Some(valid_element)
                }
            }
            Type::Union(union) => {
                if let Some(module) = TypedDictModule::from_type(db, ty) {
                    return Some(ClassBase::TypedDict(module));
                }

                // We do not support full unions of MROs (yet). Until we do,
                // support the cases where one of the types in the union is
                // a dynamic type such as `Any` or `Unknown`, and all other
                // types *would be* valid class bases. In this case, we can
                // "fold" the other potential bases into the dynamic type,
                // and return `Any`/`Unknown` as the class base to prevent
                // invalid-base diagnostics and further downstream errors.
                let Some(Type::Dynamic(dynamic)) = union
                    .elements(db)
                    .iter()
                    .find(|elem| matches!(elem, Type::Dynamic(_)))
                else {
                    return None;
                };

                if union
                    .elements(db)
                    .iter()
                    .all(|elem| ClassBase::try_from_type(env, *elem, subclass).is_some())
                {
                    Some(ClassBase::Dynamic(*dynamic))
                } else {
                    None
                }
            }
            Type::NominalInstance(_) => None, // TODO -- handle `__mro_entries__`?

            // This likely means that we're in unreachable code,
            // in which case we want to treat `Never` in a forgiving way and silence diagnostics
            Type::Never => Some(ClassBase::unknown()),

            Type::TypeAlias(alias) => Self::try_from_type(env, alias.value_type(env), subclass),

            Type::NewTypeInstance(newtype) => {
                ClassBase::try_from_type(env, newtype.concrete_base_type(env), subclass)
            }

            Type::PropertyInstance(_)
            | Type::EnumComplement(_)
            | Type::LiteralValue(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_)
            | Type::ProtocolInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_) => None,

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::SubscriptedGeneric(_) => Some(Self::Generic),
                KnownInstanceType::SubscriptedProtocol(_) => Some(Self::Protocol),
                KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::TypeVar(_)
                | KnownInstanceType::Deprecated(_)
                | KnownInstanceType::Field(_)
                | KnownInstanceType::ConstraintSet(_)
                | KnownInstanceType::ConstraintSetSolution(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::GenericContext(_)
                | KnownInstanceType::Specialization(_)
                | KnownInstanceType::UnionType(_)
                | KnownInstanceType::Literal(_)
                | KnownInstanceType::LiteralStringAlias(_)
                | KnownInstanceType::NamedTupleSpec(_)
                | KnownInstanceType::Sentinel(_)
                | KnownInstanceType::Range { .. }
                // A class inheriting from a newtype would make intuitive sense, but newtype
                // wrappers are just identity callables at runtime, so this sort of inheritance
                // doesn't work and isn't allowed.
                | KnownInstanceType::NewType(_)
                | KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::FunctoolsPartialCall(_) => None,
                KnownInstanceType::TypeGenericAlias(_) => {
                    Self::try_from_type(
                        env,
                        KnownClass::Type.to_class_literal(env),
                        subclass,
                    )
                }
                KnownInstanceType::Annotated(ty) => {
                    match ty.inner(db) {
                        Type::Dynamic(dynamic) => Some(Self::Dynamic(dynamic)),
                        Type::NominalInstance(instance) => {
                            Some(Self::Class(instance.class(env)))
                        }
                        _ => None,
                    }
                }
            },

            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::TypeQualifier(_) => None,

                SpecialFormType::Annotated
                | SpecialFormType::Literal
                | SpecialFormType::LiteralString
                | SpecialFormType::Union
                | SpecialFormType::NoReturn
                | SpecialFormType::Never
                | SpecialFormType::TypeGuard
                | SpecialFormType::TypeIs
                | SpecialFormType::TypingSelf
                | SpecialFormType::Unpack
                | SpecialFormType::Concatenate
                | SpecialFormType::TypeAlias
                | SpecialFormType::Optional
                | SpecialFormType::Not
                | SpecialFormType::Top
                | SpecialFormType::Bottom
                | SpecialFormType::Intersection
                | SpecialFormType::TypeOf
                | SpecialFormType::CallableTypeOf
                | SpecialFormType::RegularCallableTypeOf
                | SpecialFormType::Divergent
                | SpecialFormType::Todo
                | SpecialFormType::AlwaysTruthy
                | SpecialFormType::AlwaysFalsy
                | SpecialFormType::TypeForm => None,

                SpecialFormType::Any => Some(Self::Dynamic(DynamicType::Any)),
                SpecialFormType::Unknown => Some(Self::unknown()),
                SpecialFormType::Protocol => Some(Self::Protocol),
                SpecialFormType::Generic => Some(Self::Generic),
                SpecialFormType::TypedDict(module) => Some(Self::TypedDict(module)),

                SpecialFormType::NamedTuple => {
                    let class = subclass?.as_static()?;
                    let fields = class.own_fields(env, None, CodeGeneratorKind::NamedTuple);
                    Self::try_from_type(
                        env,
                        TupleType::heterogeneous(
                            db,
                            fields.values().map(|field| field.declared_ty),
                        )?
                        .to_class_type(db, env.program())
                        .into(),
                        subclass,
                    )
                }

                // TODO: Classes inheriting from `typing.Type` also have `Generic` in their MRO
                SpecialFormType::Type => {
                    Self::try_from_type(env, KnownClass::Type.to_class_literal(env), subclass)
                }

                SpecialFormType::Tuple => {
                    Self::try_from_type(env, KnownClass::Tuple.to_class_literal(env), subclass)
                }

                SpecialFormType::LegacyStdlibAlias(alias) => {
                    Self::try_from_type(env, alias.aliased_class().to_class_literal(env), subclass)
                }

                SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => {
                    Self::try_from_type(
                        env,
                        todo_type!("Support for Callable as a base class"),
                        subclass,
                    )
                }
            },
        }
    }

    pub(super) fn into_class(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(class) => Some(class),
            Self::Any
            | Self::Dynamic(_)
            | Self::Divergent(_)
            | Self::Generic
            | Self::Protocol
            | Self::TypedDict(_) => None,
        }
    }

    /// Return the metaclass of this class base.
    pub(crate) fn metaclass(self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        match self {
            Self::Class(class) => class.metaclass(env),
            Self::Any => Type::Dynamic(DynamicType::Any),
            Self::Dynamic(dynamic) => Type::Dynamic(dynamic),
            Self::Divergent(divergent) => Type::Divergent(divergent),
            // TODO: all `Protocol` classes actually have `_ProtocolMeta` as their metaclass.
            Self::Protocol | Self::Generic | Self::TypedDict(_) => {
                KnownClass::Type.to_instance(env)
            }
        }
    }

    fn apply_type_mapping_impl<'a>(
        self,
        env: &SemanticEnvironment<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::Class(class) => {
                Self::Class(class.apply_type_mapping_impl(env, type_mapping, tcx, visitor))
            }
            Self::Any
            | Self::Dynamic(_)
            | Self::Divergent(_)
            | Self::Generic
            | Self::Protocol
            | Self::TypedDict(_) => self,
        }
    }

    pub(crate) fn apply_optional_specialization(
        self,
        env: &SemanticEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        let db = env.db();
        if let Some(specialization) = specialization {
            let new_self = self.apply_type_mapping_impl(
                env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::Specialization(
                    specialization,
                )),
                TypeContext::default(),
                &ApplyTypeMappingVisitor::default(),
            );
            match specialization.materialization_kind(db) {
                None => new_self,
                Some(materialization_kind) => new_self.materialize(env, materialization_kind),
            }
        } else {
            self
        }
    }

    fn materialize(self, env: &SemanticEnvironment<'db>, kind: MaterializationKind) -> Self {
        self.apply_type_mapping_impl(
            env,
            &TypeMapping::Materialize(kind),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        )
    }

    pub(super) fn has_cyclic_mro(self, env: &SemanticEnvironment<'db>) -> bool {
        let db = env.db();
        match self {
            ClassBase::Class(class) => {
                let Some((class_literal, specialization)) = class.static_class_literal(db) else {
                    // Dynamic classes can't have cyclic MRO since their bases must
                    // already exist at creation time. Unlike statement classes, we do not
                    // permit dynamic classes to have forward references in their
                    // bases list.
                    return false;
                };
                class_literal
                    .try_mro(env, specialization)
                    .is_err_and(StaticMroError::is_cycle)
            }
            ClassBase::Any
            | ClassBase::Dynamic(_)
            | ClassBase::Divergent(_)
            | ClassBase::Generic
            | ClassBase::Protocol
            | ClassBase::TypedDict(_) => false,
        }
    }

    /// Iterate over the MRO of this base
    pub(super) fn mro(
        self,
        env: &SemanticEnvironment<'db>,
        additional_specialization: Option<Specialization<'db>>,
    ) -> impl Iterator<Item = ClassBase<'db>> + Clone {
        match self {
            ClassBase::Protocol => ClassBaseMroIterator::length_3(env, self, ClassBase::Generic),
            ClassBase::Any
            | ClassBase::Dynamic(_)
            | ClassBase::Divergent(_)
            | ClassBase::Generic
            | ClassBase::TypedDict(_) => ClassBaseMroIterator::length_2(env, self),
            ClassBase::Class(class) => {
                ClassBaseMroIterator::from_class(env, class, additional_specialization)
            }
        }
    }

    pub(super) fn display(self, env: &SemanticEnvironment<'db>) -> impl std::fmt::Display {
        self.display_with(env, DisplaySettings::default())
    }

    pub(super) fn display_with<'env>(
        self,
        env: &'env SemanticEnvironment<'db>,
        display_settings: DisplaySettings<'db>,
    ) -> impl std::fmt::Display + 'env {
        struct ClassBaseDisplay<'env, 'db> {
            env: &'env SemanticEnvironment<'db>,
            base: ClassBase<'db>,
            settings: DisplaySettings<'db>,
        }

        impl std::fmt::Display for ClassBaseDisplay<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.base {
                    ClassBase::Any => f.write_str("Any"),
                    ClassBase::Dynamic(dynamic) => dynamic.fmt(f),
                    ClassBase::Divergent(_) => f.write_str("Divergent"),
                    ClassBase::Class(class) => Type::from(class)
                        .display_with(self.env, self.settings.clone())
                        .fmt(f),
                    ClassBase::Protocol => f.write_str("typing.Protocol"),
                    ClassBase::Generic => f.write_str("typing.Generic"),
                    ClassBase::TypedDict(_) => f.write_str("typing.TypedDict"),
                }
            }
        }

        ClassBaseDisplay {
            env,
            base: self,
            settings: display_settings,
        }
    }
}

impl<'db> From<ClassType<'db>> for ClassBase<'db> {
    fn from(value: ClassType<'db>) -> Self {
        ClassBase::Class(value)
    }
}

impl<'db> From<ClassBase<'db>> for Type<'db> {
    fn from(value: ClassBase<'db>) -> Self {
        match value {
            ClassBase::Any => Type::Dynamic(DynamicType::Any),
            ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            ClassBase::Divergent(divergent) => Type::Divergent(divergent),
            ClassBase::Class(class) => class.into(),
            ClassBase::Protocol => Type::SpecialForm(SpecialFormType::Protocol),
            ClassBase::Generic => Type::SpecialForm(SpecialFormType::Generic),
            ClassBase::TypedDict(module) => Type::SpecialForm(SpecialFormType::TypedDict(module)),
        }
    }
}

impl<'db> From<&ClassBase<'db>> for Type<'db> {
    fn from(value: &ClassBase<'db>) -> Self {
        Self::from(*value)
    }
}

/// An iterator over the MRO of a class base.
#[derive(Clone)]
enum ClassBaseMroIterator<'db> {
    Length2(core::array::IntoIter<ClassBase<'db>, 2>),
    Length3(core::array::IntoIter<ClassBase<'db>, 3>),
    FromClass(MroIterator<'db>),
}

impl<'db> ClassBaseMroIterator<'db> {
    /// Iterate over an MRO of length 2 that consists of `first_element` and then `object`.
    fn length_2(env: &SemanticEnvironment<'db>, first_element: ClassBase<'db>) -> Self {
        ClassBaseMroIterator::Length2([first_element, ClassBase::object(env)].into_iter())
    }

    /// Iterate over an MRO of length 3 that consists of `first_element`, then `second_element`, then `object`.
    fn length_3(
        env: &SemanticEnvironment<'db>,
        element_1: ClassBase<'db>,
        element_2: ClassBase<'db>,
    ) -> Self {
        ClassBaseMroIterator::Length3([element_1, element_2, ClassBase::object(env)].into_iter())
    }

    /// Iterate over the MRO of an arbitrary class. The MRO may be of any length.
    fn from_class(
        env: &SemanticEnvironment<'db>,
        class: ClassType<'db>,
        additional_specialization: Option<Specialization<'db>>,
    ) -> Self {
        ClassBaseMroIterator::FromClass(class.iter_mro_specialized(env, additional_specialization))
    }
}

impl<'db> Iterator for ClassBaseMroIterator<'db> {
    type Item = ClassBase<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Length2(iter) => iter.next(),
            Self::Length3(iter) => iter.next(),
            Self::FromClass(iter) => iter.next(),
        }
    }
}

impl std::iter::FusedIterator for ClassBaseMroIterator<'_> {}
