use crate::types::{
    todo_type, Class, ClassLiteralType, KnownClass, KnownInstanceType, TodoType, Type,
};
use crate::Db;
use itertools::Either;

/// Enumeration of the possible kinds of types we allow in class bases.
///
/// This is much more limited than the [`Type`] enum:
/// all types that would be invalid to have as a class base are
/// transformed into [`ClassBase::Unknown`]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update)]
pub enum ClassBase<'db> {
    Any,
    Unknown,
    Todo(TodoType),
    Class(Class<'db>),
}

impl<'db> ClassBase<'db> {
    pub fn display(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        struct Display<'db> {
            base: ClassBase<'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.base {
                    ClassBase::Any => f.write_str("Any"),
                    ClassBase::Todo(todo) => todo.fmt(f),
                    ClassBase::Unknown => f.write_str("Unknown"),
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
            .map_or(Self::Unknown, |ClassLiteralType { class }| {
                Self::Class(class)
            })
    }

    /// Attempt to resolve `ty` into a `ClassBase`.
    ///
    /// Return `None` if `ty` is not an acceptable type for a class base.
    pub(super) fn try_from_ty(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Any => Some(Self::Any),
            Type::Unknown => Some(Self::Unknown),
            Type::Todo(todo) => Some(Self::Todo(todo)),
            Type::ClassLiteral(ClassLiteralType { class }) => Some(Self::Class(class)),
            Type::Union(_) => None, // TODO -- forces consideration of multiple possible MROs?
            Type::Intersection(_) => None, // TODO -- probably incorrect?
            Type::Instance(_) => None, // TODO -- handle `__mro_entries__`?
            Type::Never
            | Type::BooleanLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::SliceLiteral(_)
            | Type::ModuleLiteral(_)
            | Type::SubclassOf(_) => None,
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
                | KnownInstanceType::Optional => None,
                KnownInstanceType::Any => Some(Self::Any),
                // TODO: Classes inheriting from `typing.Type` et al. also have `Generic` in their MRO
                KnownInstanceType::Dict => {
                    Self::try_from_ty(db, KnownClass::Dict.to_class_literal(db))
                }
                KnownInstanceType::List => {
                    Self::try_from_ty(db, KnownClass::List.to_class_literal(db))
                }
                KnownInstanceType::Type => {
                    Self::try_from_ty(db, KnownClass::Type.to_class_literal(db))
                }
                KnownInstanceType::Tuple => {
                    Self::try_from_ty(db, KnownClass::Tuple.to_class_literal(db))
                }
                KnownInstanceType::Set => {
                    Self::try_from_ty(db, KnownClass::Set.to_class_literal(db))
                }
                KnownInstanceType::FrozenSet => {
                    Self::try_from_ty(db, KnownClass::FrozenSet.to_class_literal(db))
                }
                KnownInstanceType::Callable
                | KnownInstanceType::ChainMap
                | KnownInstanceType::Counter
                | KnownInstanceType::DefaultDict
                | KnownInstanceType::Deque
                | KnownInstanceType::OrderedDict => Self::try_from_ty(
                    db,
                    todo_type!("Support for more typing aliases as base classes"),
                ),
            },
        }
    }

    pub(super) fn into_class(self) -> Option<Class<'db>> {
        match self {
            Self::Class(class) => Some(class),
            _ => None,
        }
    }

    /// Iterate over the MRO of this base
    pub(super) fn mro(
        self,
        db: &'db dyn Db,
    ) -> Either<impl Iterator<Item = ClassBase<'db>>, impl Iterator<Item = ClassBase<'db>>> {
        match self {
            ClassBase::Any => Either::Left([ClassBase::Any, ClassBase::object(db)].into_iter()),
            ClassBase::Unknown => {
                Either::Left([ClassBase::Unknown, ClassBase::object(db)].into_iter())
            }
            ClassBase::Todo(todo) => {
                Either::Left([ClassBase::Todo(todo), ClassBase::object(db)].into_iter())
            }
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
            ClassBase::Any => Type::Any,
            ClassBase::Todo(todo) => Type::Todo(todo),
            ClassBase::Unknown => Type::Unknown,
            ClassBase::Class(class) => Type::class_literal(class),
        }
    }
}

impl<'db> From<&ClassBase<'db>> for Type<'db> {
    fn from(value: &ClassBase<'db>) -> Self {
        Self::from(*value)
    }
}
