use crate::types::{
    todo_type, ClassType, DynamicType, KnownClass, KnownInstanceType, MroIterator, Type,
};
use crate::Db;

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum: all types that would be invalid to have as a
/// class base are transformed into [`ClassBase::unknown()`]
///
/// Note that a non-specialized generic class _cannot_ be a class base. When we see a
/// non-specialized generic class in any type expression (including the list of base classes), we
/// automatically construct the default specialization for that class.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update)]
pub enum ClassBase<'db> {
    Dynamic(DynamicType),
    Class(ClassType<'db>),
    /// Although `Protocol` is not a class in typeshed's stubs, it is at runtime,
    /// and can appear in the MRO of a class.
    Protocol,
    /// Bare `Generic` cannot be subclassed directly in user code,
    /// but nonetheless appears in the MRO of classes that inherit from `Generic[T]`,
    /// `Protocol[T]`, or bare `Protocol`.
    Generic,
}

impl<'db> ClassBase<'db> {
    pub(crate) const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) fn display(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        struct Display<'db> {
            base: ClassBase<'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.base {
                    ClassBase::Dynamic(dynamic) => dynamic.fmt(f),
                    ClassBase::Class(class @ ClassType::NonGeneric(_)) => {
                        write!(f, "<class '{}'>", class.name(self.db))
                    }
                    ClassBase::Class(ClassType::Generic(alias)) => {
                        write!(f, "<class '{}'>", alias.display(self.db))
                    }
                    ClassBase::Protocol => f.write_str("typing.Protocol"),
                    ClassBase::Generic => f.write_str("typing.Generic"),
                }
            }
        }

        Display { base: self, db }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class_literal(db)
            .to_class_type(db)
            .map_or(Self::unknown(), Self::Class)
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    pub(super) fn try_from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Dynamic(dynamic) => Some(Self::Dynamic(dynamic)),
            Type::ClassLiteral(literal) => Some(if literal.is_known(db, KnownClass::Any) {
                Self::Dynamic(DynamicType::Any)
            } else {
                Self::Class(literal.default_specialization(db))
            }),
            Type::GenericAlias(generic) => Some(Self::Class(ClassType::Generic(generic))),
            Type::NominalInstance(instance)
                if instance.class().is_known(db, KnownClass::GenericAlias) =>
            {
                Self::try_from_type(db, todo_type!("GenericAlias instance"))
            }
            Type::Union(_) => None, // TODO -- forces consideration of multiple possible MROs?
            Type::Intersection(_) => None, // TODO -- probably incorrect?
            Type::NominalInstance(_) => None, // TODO -- handle `__mro_entries__`?
            Type::PropertyInstance(_) => None,
            Type::Never
            | Type::BooleanLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::SliceLiteral(_)
            | Type::ModuleLiteral(_)
            | Type::SubclassOf(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_)
            | Type::ProtocolInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy => None,
            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::TypeVar(_)
                | KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::Annotated
                | KnownInstanceType::Literal
                | KnownInstanceType::LiteralString
                | KnownInstanceType::Union
                | KnownInstanceType::NoReturn
                | KnownInstanceType::Never
                | KnownInstanceType::Final
                | KnownInstanceType::NotRequired
                | KnownInstanceType::TypeGuard
                | KnownInstanceType::TypeIs
                | KnownInstanceType::TypingSelf
                | KnownInstanceType::Unpack
                | KnownInstanceType::ClassVar
                | KnownInstanceType::Concatenate
                | KnownInstanceType::Required
                | KnownInstanceType::TypeAlias
                | KnownInstanceType::ReadOnly
                | KnownInstanceType::Optional
                | KnownInstanceType::Not
                | KnownInstanceType::Intersection
                | KnownInstanceType::TypeOf
                | KnownInstanceType::CallableTypeOf
                | KnownInstanceType::AlwaysTruthy
                | KnownInstanceType::AlwaysFalsy => None,
                KnownInstanceType::Unknown => Some(Self::unknown()),
                KnownInstanceType::Any => Some(Self::any()),
                // TODO: Classes inheriting from `typing.Type` et al. also have `Generic` in their MRO
                KnownInstanceType::Dict => {
                    Self::try_from_type(db, KnownClass::Dict.to_class_literal(db))
                }
                KnownInstanceType::List => {
                    Self::try_from_type(db, KnownClass::List.to_class_literal(db))
                }
                KnownInstanceType::Type => {
                    Self::try_from_type(db, KnownClass::Type.to_class_literal(db))
                }
                KnownInstanceType::Tuple => {
                    Self::try_from_type(db, KnownClass::Tuple.to_class_literal(db))
                }
                KnownInstanceType::Set => {
                    Self::try_from_type(db, KnownClass::Set.to_class_literal(db))
                }
                KnownInstanceType::FrozenSet => {
                    Self::try_from_type(db, KnownClass::FrozenSet.to_class_literal(db))
                }
                KnownInstanceType::ChainMap => {
                    Self::try_from_type(db, KnownClass::ChainMap.to_class_literal(db))
                }
                KnownInstanceType::Counter => {
                    Self::try_from_type(db, KnownClass::Counter.to_class_literal(db))
                }
                KnownInstanceType::DefaultDict => {
                    Self::try_from_type(db, KnownClass::DefaultDict.to_class_literal(db))
                }
                KnownInstanceType::Deque => {
                    Self::try_from_type(db, KnownClass::Deque.to_class_literal(db))
                }
                KnownInstanceType::OrderedDict => {
                    Self::try_from_type(db, KnownClass::OrderedDict.to_class_literal(db))
                }
                KnownInstanceType::TypedDict => Self::try_from_type(db, todo_type!("TypedDict")),
                KnownInstanceType::Callable => {
                    Self::try_from_type(db, todo_type!("Support for Callable as a base class"))
                }
                KnownInstanceType::Protocol => Some(ClassBase::Protocol),
                KnownInstanceType::Generic => Some(ClassBase::Generic),
            },
        }
    }

    pub(super) fn into_class(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(class) => Some(class),
            Self::Dynamic(_) | Self::Generic | Self::Protocol => None,
        }
    }

    /// Iterate over the MRO of this base
    pub(super) fn mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        match self {
            ClassBase::Protocol => ClassBaseMroIterator::length_3(db, self, ClassBase::Generic),
            ClassBase::Dynamic(DynamicType::SubscriptedProtocol) => ClassBaseMroIterator::length_3(
                db,
                self,
                ClassBase::Dynamic(DynamicType::SubscriptedGeneric),
            ),
            ClassBase::Dynamic(_) | ClassBase::Generic => ClassBaseMroIterator::length_2(db, self),
            ClassBase::Class(class) => ClassBaseMroIterator::from_class(db, class),
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
            ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            ClassBase::Class(class) => class.into(),
            ClassBase::Protocol => Type::KnownInstance(KnownInstanceType::Protocol),
            ClassBase::Generic => Type::KnownInstance(KnownInstanceType::Generic),
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
    fn from_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        ClassBaseMroIterator::FromClass(class.iter_mro(db))
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
