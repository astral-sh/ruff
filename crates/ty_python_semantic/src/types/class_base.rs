use crate::Db;
use crate::types::class::CodeGeneratorKind;
use crate::types::generics::Specialization;
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingVisitor, ClassLiteral, ClassType, DynamicType, KnownClass, KnownInstanceType,
    MaterializationKind, MroError, MroIterator, NormalizedVisitor, SpecialFormType, Type,
    TypeContext, TypeMapping, todo_type,
};

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum: all types that would be invalid to have as a
/// class base are transformed into [`ClassBase::unknown()`]
///
/// Note that a non-specialized generic class _cannot_ be a class base. When we see a
/// non-specialized generic class in any type expression (including the list of base classes), we
/// automatically construct the default specialization for that class.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum ClassBase<'db> {
    Dynamic(DynamicType<'db>),
    Class(ClassType<'db>),
    /// Although `Protocol` is not a class in typeshed's stubs, it is at runtime,
    /// and can appear in the MRO of a class.
    Protocol,
    /// Bare `Generic` cannot be subclassed directly in user code,
    /// but nonetheless appears in the MRO of classes that inherit from `Generic[T]`,
    /// `Protocol[T]`, or bare `Protocol`.
    Generic,
    TypedDict,
}

impl<'db> ClassBase<'db> {
    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            Self::Dynamic(dynamic) => Self::Dynamic(dynamic.normalized()),
            Self::Class(class) => Self::Class(class.normalized_impl(db, visitor)),
            Self::Protocol | Self::Generic | Self::TypedDict => self,
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Dynamic(dynamic) => Some(Self::Dynamic(dynamic.recursive_type_normalized())),
            Self::Class(class) => Some(Self::Class(
                class.recursive_type_normalized_impl(db, div, nested)?,
            )),
            Self::Protocol | Self::Generic | Self::TypedDict => Some(self),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            ClassBase::Class(class) => class.name(db),
            ClassBase::Dynamic(DynamicType::Any) => "Any",
            ClassBase::Dynamic(DynamicType::Unknown | DynamicType::UnknownGeneric(_)) => "Unknown",
            ClassBase::Dynamic(
                DynamicType::Todo(_) | DynamicType::TodoUnpack | DynamicType::TodoStarredExpression,
            ) => "@Todo",
            ClassBase::Dynamic(DynamicType::Divergent(_)) => "Divergent",
            ClassBase::Protocol => "Protocol",
            ClassBase::Generic => "Generic",
            ClassBase::TypedDict => "TypedDict",
        }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class_literal(db)
            .to_class_type(db)
            .map_or(Self::unknown(), Self::Class)
    }

    pub(super) const fn is_typed_dict(self) -> bool {
        matches!(self, ClassBase::TypedDict)
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    pub(super) fn try_from_type(
        db: &'db dyn Db,
        ty: Type<'db>,
        subclass: ClassLiteral<'db>,
    ) -> Option<Self> {
        match ty {
            Type::Dynamic(dynamic) => Some(Self::Dynamic(dynamic)),
            Type::ClassLiteral(literal) => Some(Self::Class(literal.default_specialization(db))),
            Type::GenericAlias(generic) => Some(Self::Class(ClassType::Generic(generic))),
            Type::NominalInstance(instance)
                if instance.has_known_class(db, KnownClass::GenericAlias) =>
            {
                Self::try_from_type(db, todo_type!("GenericAlias instance"), subclass)
            }
            Type::SubclassOf(subclass_of) => subclass_of
                .subclass_of()
                .into_dynamic()
                .map(ClassBase::Dynamic),
            Type::Intersection(inter) => {
                let valid_element = inter
                    .positive(db)
                    .iter()
                    .find_map(|elem| ClassBase::try_from_type(db, *elem, subclass))?;

                if ty.is_disjoint_from(db, KnownClass::Type.to_instance(db)) {
                    None
                } else {
                    Some(valid_element)
                }
            }
            Type::Union(union) => {
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
                    .all(|elem| ClassBase::try_from_type(db, *elem, subclass).is_some())
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

            Type::TypeAlias(alias) => Self::try_from_type(db, alias.value_type(db), subclass),

            Type::NewTypeInstance(newtype) => {
                ClassBase::try_from_type(db, newtype.concrete_base_type(db), subclass)
            }

            Type::PropertyInstance(_)
            | Type::BooleanLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::EnumLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::ModuleLiteral(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_)
            | Type::ProtocolInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::TypeIs(_)
            | Type::TypedDict(_) => None,

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::SubscriptedGeneric(_) => Some(Self::Generic),
                KnownInstanceType::SubscriptedProtocol(_) => Some(Self::Protocol),
                KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::TypeVar(_)
                | KnownInstanceType::Deprecated(_)
                | KnownInstanceType::Field(_)
                | KnownInstanceType::ConstraintSet(_)
                | KnownInstanceType::Callable(_)
                | KnownInstanceType::GenericContext(_)
                | KnownInstanceType::Specialization(_)
                | KnownInstanceType::UnionType(_)
                | KnownInstanceType::Literal(_)
                | KnownInstanceType::LiteralStringAlias(_)
                // A class inheriting from a newtype would make intuitive sense, but newtype
                // wrappers are just identity callables at runtime, so this sort of inheritance
                // doesn't work and isn't allowed.
                | KnownInstanceType::NewType(_) => None,
                KnownInstanceType::TypeGenericAlias(_) => {
                    Self::try_from_type(db, KnownClass::Type.to_class_literal(db), subclass)
                }
                KnownInstanceType::Annotated(ty) => {
                    // Unions are not supported in this position, so we only need to support
                    // something like `class C(Annotated[Base, "metadata"]): ...`, which we
                    // can do by turning the instance type (`Base` in this example) back into
                    // a class.
                    let annotated_ty = ty.inner(db);
                    let instance_ty = annotated_ty.as_nominal_instance()?;

                    Some(Self::Class(instance_ty.class(db)))
                }
            },

            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::Annotated
                | SpecialFormType::Literal
                | SpecialFormType::LiteralString
                | SpecialFormType::Union
                | SpecialFormType::NoReturn
                | SpecialFormType::Never
                | SpecialFormType::Final
                | SpecialFormType::NotRequired
                | SpecialFormType::TypeGuard
                | SpecialFormType::TypeIs
                | SpecialFormType::TypingSelf
                | SpecialFormType::Unpack
                | SpecialFormType::ClassVar
                | SpecialFormType::Concatenate
                | SpecialFormType::Required
                | SpecialFormType::TypeAlias
                | SpecialFormType::ReadOnly
                | SpecialFormType::Optional
                | SpecialFormType::Not
                | SpecialFormType::Top
                | SpecialFormType::Bottom
                | SpecialFormType::Intersection
                | SpecialFormType::TypeOf
                | SpecialFormType::CallableTypeOf
                | SpecialFormType::AlwaysTruthy
                | SpecialFormType::AlwaysFalsy => None,

                SpecialFormType::Any => Some(Self::Dynamic(DynamicType::Any)),
                SpecialFormType::Unknown => Some(Self::unknown()),

                SpecialFormType::Protocol => Some(Self::Protocol),
                SpecialFormType::Generic => Some(Self::Generic),

                SpecialFormType::NamedTuple => {
                    let fields = subclass.own_fields(db, None, CodeGeneratorKind::NamedTuple);
                    Self::try_from_type(
                        db,
                        TupleType::heterogeneous(
                            db,
                            fields.values().map(|field| field.declared_ty),
                        )?
                        .to_class_type(db)
                        .into(),
                        subclass,
                    )
                }

                // TODO: Classes inheriting from `typing.Type` et al. also have `Generic` in their MRO
                SpecialFormType::Dict => {
                    Self::try_from_type(db, KnownClass::Dict.to_class_literal(db), subclass)
                }
                SpecialFormType::List => {
                    Self::try_from_type(db, KnownClass::List.to_class_literal(db), subclass)
                }
                SpecialFormType::Type => {
                    Self::try_from_type(db, KnownClass::Type.to_class_literal(db), subclass)
                }
                SpecialFormType::Tuple => {
                    Self::try_from_type(db, KnownClass::Tuple.to_class_literal(db), subclass)
                }
                SpecialFormType::Set => {
                    Self::try_from_type(db, KnownClass::Set.to_class_literal(db), subclass)
                }
                SpecialFormType::FrozenSet => {
                    Self::try_from_type(db, KnownClass::FrozenSet.to_class_literal(db), subclass)
                }
                SpecialFormType::ChainMap => {
                    Self::try_from_type(db, KnownClass::ChainMap.to_class_literal(db), subclass)
                }
                SpecialFormType::Counter => {
                    Self::try_from_type(db, KnownClass::Counter.to_class_literal(db), subclass)
                }
                SpecialFormType::DefaultDict => {
                    Self::try_from_type(db, KnownClass::DefaultDict.to_class_literal(db), subclass)
                }
                SpecialFormType::Deque => {
                    Self::try_from_type(db, KnownClass::Deque.to_class_literal(db), subclass)
                }
                SpecialFormType::OrderedDict => {
                    Self::try_from_type(db, KnownClass::OrderedDict.to_class_literal(db), subclass)
                }
                SpecialFormType::TypedDict => Some(Self::TypedDict),
                SpecialFormType::Callable => Self::try_from_type(
                    db,
                    todo_type!("Support for Callable as a base class"),
                    subclass,
                ),
            },
        }
    }

    pub(super) fn into_class(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(class) => Some(class),
            Self::Dynamic(_) | Self::Generic | Self::Protocol | Self::TypedDict => None,
        }
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::Class(class) => {
                Self::Class(class.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Self::Dynamic(_) | Self::Generic | Self::Protocol | Self::TypedDict => self,
        }
    }

    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        if let Some(specialization) = specialization {
            let new_self = self.apply_type_mapping_impl(
                db,
                &TypeMapping::Specialization(specialization),
                TypeContext::default(),
                &ApplyTypeMappingVisitor::default(),
            );
            match specialization.materialization_kind(db) {
                None => new_self,
                Some(materialization_kind) => new_self.materialize(db, materialization_kind),
            }
        } else {
            self
        }
    }

    fn materialize(self, db: &'db dyn Db, kind: MaterializationKind) -> Self {
        self.apply_type_mapping_impl(
            db,
            &TypeMapping::Materialize(kind),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        )
    }

    pub(super) fn has_cyclic_mro(self, db: &'db dyn Db) -> bool {
        match self {
            ClassBase::Class(class) => {
                let (class_literal, specialization) = class.class_literal(db);
                class_literal
                    .try_mro(db, specialization)
                    .is_err_and(MroError::is_cycle)
            }
            ClassBase::Dynamic(_)
            | ClassBase::Generic
            | ClassBase::Protocol
            | ClassBase::TypedDict => false,
        }
    }

    /// Iterate over the MRO of this base
    pub(super) fn mro(
        self,
        db: &'db dyn Db,
        additional_specialization: Option<Specialization<'db>>,
    ) -> impl Iterator<Item = ClassBase<'db>> {
        match self {
            ClassBase::Protocol => ClassBaseMroIterator::length_3(db, self, ClassBase::Generic),
            ClassBase::Dynamic(_) | ClassBase::Generic | ClassBase::TypedDict => {
                ClassBaseMroIterator::length_2(db, self)
            }
            ClassBase::Class(class) => {
                ClassBaseMroIterator::from_class(db, class, additional_specialization)
            }
        }
    }

    pub(super) fn display(self, db: &'db dyn Db) -> impl std::fmt::Display {
        struct ClassBaseDisplay<'db> {
            db: &'db dyn Db,
            base: ClassBase<'db>,
        }

        impl std::fmt::Display for ClassBaseDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.base {
                    ClassBase::Dynamic(dynamic) => dynamic.fmt(f),
                    ClassBase::Class(class) => Type::from(class).display(self.db).fmt(f),
                    ClassBase::Protocol => f.write_str("typing.Protocol"),
                    ClassBase::Generic => f.write_str("typing.Generic"),
                    ClassBase::TypedDict => f.write_str("typing.TypedDict"),
                }
            }
        }

        ClassBaseDisplay { db, base: self }
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
            ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            ClassBase::Class(class) => class.into(),
            ClassBase::Protocol => Type::SpecialForm(SpecialFormType::Protocol),
            ClassBase::Generic => Type::SpecialForm(SpecialFormType::Generic),
            ClassBase::TypedDict => Type::SpecialForm(SpecialFormType::TypedDict),
        }
    }
}

impl<'db> From<&ClassBase<'db>> for Type<'db> {
    fn from(value: &ClassBase<'db>) -> Self {
        Self::from(*value)
    }
}

/// An iterator over the MRO of a class base.
enum ClassBaseMroIterator<'db> {
    Length2(core::array::IntoIter<ClassBase<'db>, 2>),
    Length3(core::array::IntoIter<ClassBase<'db>, 3>),
    FromClass(MroIterator<'db>),
}

impl<'db> ClassBaseMroIterator<'db> {
    /// Iterate over an MRO of length 2 that consists of `first_element` and then `object`.
    fn length_2(db: &'db dyn Db, first_element: ClassBase<'db>) -> Self {
        ClassBaseMroIterator::Length2([first_element, ClassBase::object(db)].into_iter())
    }

    /// Iterate over an MRO of length 3 that consists of `first_element`, then `second_element`, then `object`.
    fn length_3(db: &'db dyn Db, element_1: ClassBase<'db>, element_2: ClassBase<'db>) -> Self {
        ClassBaseMroIterator::Length3([element_1, element_2, ClassBase::object(db)].into_iter())
    }

    /// Iterate over the MRO of an arbitrary class. The MRO may be of any length.
    fn from_class(
        db: &'db dyn Db,
        class: ClassType<'db>,
        additional_specialization: Option<Specialization<'db>>,
    ) -> Self {
        ClassBaseMroIterator::FromClass(class.iter_mro_specialized(db, additional_specialization))
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
