use crate::symbol::SymbolAndQualifiers;

use super::{ClassType, Db, DynamicType, KnownClass, MemberLookupPolicy, Type};

/// A type that represents `type[C]`, i.e. the class object `C` and class objects that are subclasses of `C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct SubclassOfType<'db> {
    // Keep this field private, so that the only way of constructing the struct is through the `from` method.
    subclass_of: SubclassOfInner<'db>,
}

impl<'db> SubclassOfType<'db> {
    /// Construct a new [`Type`] instance representing a given class object (or a given dynamic type)
    /// and all possible subclasses of that class object/dynamic type.
    ///
    /// This method does not always return a [`Type::SubclassOf`] variant.
    /// If the class object is known to be a final class,
    /// this method will return a [`Type::ClassLiteral`] variant; this is a more precise type.
    /// If the class object is `builtins.object`, `Type::NominalInstance(<builtins.type>)`
    /// will be returned; this is no more precise, but it is exactly equivalent to `type[object]`.
    ///
    /// The eager normalization here means that we do not need to worry elsewhere about distinguishing
    /// between `@final` classes and other classes when dealing with [`Type::SubclassOf`] variants.
    pub(crate) fn from(db: &'db dyn Db, subclass_of: impl Into<SubclassOfInner<'db>>) -> Type<'db> {
        let subclass_of = subclass_of.into();
        match subclass_of {
            SubclassOfInner::Dynamic(_) => Type::SubclassOf(Self { subclass_of }),
            SubclassOfInner::Class(class) => {
                if class.is_final(db) {
                    Type::from(class)
                } else if class.is_object(db) {
                    KnownClass::Type.to_instance(db)
                } else {
                    Type::SubclassOf(Self { subclass_of })
                }
            }
        }
    }

    /// Return a [`Type`] instance representing the type `type[Unknown]`.
    pub(crate) const fn subclass_of_unknown() -> Type<'db> {
        Type::SubclassOf(SubclassOfType {
            subclass_of: SubclassOfInner::unknown(),
        })
    }

    /// Return a [`Type`] instance representing the type `type[Any]`.
    pub(crate) const fn subclass_of_any() -> Type<'db> {
        Type::SubclassOf(SubclassOfType {
            subclass_of: SubclassOfInner::Dynamic(DynamicType::Any),
        })
    }

    /// Return the inner [`SubclassOfInner`] value wrapped by this `SubclassOfType`.
    pub(crate) const fn subclass_of(self) -> SubclassOfInner<'db> {
        self.subclass_of
    }

    pub(crate) const fn is_dynamic(self) -> bool {
        // Unpack `self` so that we're forced to update this method if any more fields are added in the future.
        let Self { subclass_of } = self;
        subclass_of.is_dynamic()
    }

    pub(crate) const fn is_fully_static(self) -> bool {
        !self.is_dynamic()
    }

    pub(crate) fn find_name_in_mro_with_policy(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<SymbolAndQualifiers<'db>> {
        Type::from(self.subclass_of).find_name_in_mro_with_policy(db, name, policy)
    }

    /// Return `true` if `self` is a subtype of `other`.
    ///
    /// This can only return `true` if `self.subclass_of` is a [`SubclassOfInner::Class`] variant;
    /// only fully static types participate in subtyping.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: SubclassOfType<'db>) -> bool {
        match (self.subclass_of, other.subclass_of) {
            // Non-fully-static types do not participate in subtyping
            (SubclassOfInner::Dynamic(_), _) | (_, SubclassOfInner::Dynamic(_)) => false,

            // For example, `type[bool]` describes all possible runtime subclasses of the class `bool`,
            // and `type[int]` describes all possible runtime subclasses of the class `int`.
            // The first set is a subset of the second set, because `bool` is itself a subclass of `int`.
            (SubclassOfInner::Class(self_class), SubclassOfInner::Class(other_class)) => {
                // N.B. The subclass relation is fully static
                self_class.is_subclass_of(db, other_class)
            }
        }
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        match self.subclass_of {
            SubclassOfInner::Class(class) => Type::instance(db, class),
            SubclassOfInner::Dynamic(dynamic_type) => Type::Dynamic(dynamic_type),
        }
    }
}

/// An enumeration of the different kinds of `type[]` types that a [`SubclassOfType`] can represent:
///
/// 1. A "subclass of a class": `type[C]` for any class object `C`
/// 2. A "subclass of a dynamic type": `type[Any]`, `type[Unknown]` and `type[@Todo]`
///
/// In the long term, we may want to implement <https://github.com/astral-sh/ruff/issues/15381>.
/// Doing this would allow us to get rid of this enum,
/// since `type[Any]` would be represented as `type & Any`
/// rather than using the [`Type::SubclassOf`] variant at all;
/// [`SubclassOfType`] would then be a simple wrapper around [`ClassType`].
///
/// Note that this enum is similar to the [`super::ClassBase`] enum,
/// but does not include the `ClassBase::Protocol` and `ClassBase::Generic` variants
/// (`type[Protocol]` and `type[Generic]` are not valid types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) enum SubclassOfInner<'db> {
    Class(ClassType<'db>),
    Dynamic(DynamicType),
}

impl<'db> SubclassOfInner<'db> {
    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) const fn is_dynamic(self) -> bool {
        matches!(self, Self::Dynamic(_))
    }

    pub(crate) const fn into_class(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(class) => Some(class),
            Self::Dynamic(_) => None,
        }
    }

    pub(crate) fn try_from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Dynamic(dynamic) => Some(Self::Dynamic(dynamic)),
            Type::ClassLiteral(literal) => Some(if literal.is_known(db, KnownClass::Any) {
                Self::Dynamic(DynamicType::Any)
            } else {
                Self::Class(literal.default_specialization(db))
            }),
            Type::GenericAlias(generic) => Some(Self::Class(ClassType::Generic(generic))),
            _ => None,
        }
    }
}

impl<'db> From<ClassType<'db>> for SubclassOfInner<'db> {
    fn from(value: ClassType<'db>) -> Self {
        SubclassOfInner::Class(value)
    }
}

impl<'db> From<SubclassOfInner<'db>> for Type<'db> {
    fn from(value: SubclassOfInner<'db>) -> Self {
        match value {
            SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SubclassOfInner::Class(class) => class.into(),
        }
    }
}
