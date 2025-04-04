use crate::types::{todo_type, Class, DynamicType, KnownClass, KnownInstanceType, Type};
use crate::Db;
use itertools::Either;

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum:
/// all types that would be invalid to have as a class base are
/// transformed into [`ClassBase::unknown`]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update)]
pub(crate) enum ClassBase<'db> {
    Dynamic(DynamicType),
    Class(Class<'db>),
}

impl<'db> ClassBase<'db> {
    pub(crate) const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) const fn is_dynamic(self) -> bool {
        match self {
            ClassBase::Dynamic(_) => true,
            ClassBase::Class(_) => false,
        }
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
                    ClassBase::Class(class) => write!(f, "<class '{}'>", class.name(self.db)),
                }
            }
        }

        Display { base: self, db }
    }

    /// Return a `ClassBase` representing the class `builtins.object`
    pub(super) fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object
            .to_class_literal(db)
            .into_class_literal()
            .map_or(Self::unknown(), |literal| Self::Class(literal.class()))
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    pub(super) fn try_from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Dynamic(dynamic) => Some(Self::Dynamic(dynamic)),
            Type::ClassLiteral(literal) => Some(if literal.class().is_known(db, KnownClass::Any) {
                Self::Dynamic(DynamicType::Any)
            } else {
                Self::Class(literal.class())
            }),
            Type::Union(_) => None, // TODO -- forces consideration of multiple possible MROs?
            Type::Intersection(_) => None, // TODO -- probably incorrect?
            Type::Instance(_) => None, // TODO -- handle `__mro_entries__`?
            Type::PropertyInstance(_) => None,
            Type::Never
            | Type::BooleanLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::SliceLiteral(_)
            | Type::ModuleLiteral(_)
            | Type::SubclassOf(_)
            | Type::TypeVar(_)
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
                KnownInstanceType::Callable => {
                    Self::try_from_type(db, todo_type!("Support for Callable as a base class"))
                }
                KnownInstanceType::Protocol => Some(ClassBase::Dynamic(DynamicType::TodoProtocol)),
            },
        }
    }

    pub(super) fn into_class(self) -> Option<Class<'db>> {
        match self {
            Self::Class(class) => Some(class),
            Self::Dynamic(_) => None,
        }
    }

    /// Iterate over the MRO of this base
    pub(super) fn mro(
        self,
        db: &'db dyn Db,
    ) -> Either<impl Iterator<Item = ClassBase<'db>>, impl Iterator<Item = ClassBase<'db>>> {
        match self {
            ClassBase::Dynamic(_) => Either::Left([self, ClassBase::object(db)].into_iter()),
            ClassBase::Class(class) => Either::Right(class.iter_mro(db)),
        }
    }
}

impl<'db> From<Class<'db>> for ClassBase<'db> {
    fn from(value: Class<'db>) -> Self {
        ClassBase::Class(value)
    }
}

impl<'db> From<ClassBase<'db>> for Type<'db> {
    fn from(value: ClassBase<'db>) -> Self {
        match value {
            ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            ClassBase::Class(class) => Type::class_literal(class),
        }
    }
}

impl<'db> From<&ClassBase<'db>> for Type<'db> {
    fn from(value: &ClassBase<'db>) -> Self {
        Self::from(*value)
    }
}
