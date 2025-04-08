use crate::symbol::SymbolAndQualifiers;

use super::{ClassBase, ClassLiteralType, Db, KnownClass, Type};

/// A type that represents `type[C]`, i.e. the class object `C` and class objects that are subclasses of `C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct SubclassOfType<'db> {
    // Keep this field private, so that the only way of constructing the struct is through the `from` method.
    subclass_of: ClassBase<'db>,
}

impl<'db> SubclassOfType<'db> {
    /// Construct a new [`Type`] instance representing a given class object (or a given dynamic type)
    /// and all possible subclasses of that class object/dynamic type.
    ///
    /// This method does not always return a [`Type::SubclassOf`] variant.
    /// If the class object is known to be a final class,
    /// this method will return a [`Type::ClassLiteral`] variant; this is a more precise type.
    /// If the class object is `builtins.object`, `Type::Instance(<builtins.type>)` will be returned;
    /// this is no more precise, but it is exactly equivalent to `type[object]`.
    ///
    /// The eager normalization here means that we do not need to worry elsewhere about distinguishing
    /// between `@final` classes and other classes when dealing with [`Type::SubclassOf`] variants.
    pub(crate) fn from(db: &'db dyn Db, subclass_of: impl Into<ClassBase<'db>>) -> Type<'db> {
        let subclass_of = subclass_of.into();
        match subclass_of {
            ClassBase::Dynamic(_) => Type::SubclassOf(Self { subclass_of }),
            ClassBase::Class(class) => {
                if class.is_final(db) {
                    Type::ClassLiteral(ClassLiteralType { class })
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
            subclass_of: ClassBase::unknown(),
        })
    }

    /// Return a [`Type`] instance representing the type `type[Any]`.
    pub(crate) const fn subclass_of_any() -> Type<'db> {
        Type::SubclassOf(SubclassOfType {
            subclass_of: ClassBase::any(),
        })
    }

    /// Return the inner [`ClassBase`] value wrapped by this `SubclassOfType`.
    pub(crate) const fn subclass_of(self) -> ClassBase<'db> {
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

    pub(crate) fn find_name_in_mro(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<SymbolAndQualifiers<'db>> {
        Type::from(self.subclass_of).find_name_in_mro(db, name)
    }

    /// Return `true` if `self` is a subtype of `other`.
    ///
    /// This can only return `true` if `self.subclass_of` is a [`ClassBase::Class`] variant;
    /// only fully static types participate in subtyping.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: SubclassOfType<'db>) -> bool {
        match (self.subclass_of, other.subclass_of) {
            // Non-fully-static types do not participate in subtyping
            (ClassBase::Dynamic(_), _) | (_, ClassBase::Dynamic(_)) => false,

            // For example, `type[bool]` describes all possible runtime subclasses of the class `bool`,
            // and `type[int]` describes all possible runtime subclasses of the class `int`.
            // The first set is a subset of the second set, because `bool` is itself a subclass of `int`.
            (ClassBase::Class(self_class), ClassBase::Class(other_class)) => {
                // N.B. The subclass relation is fully static
                self_class.is_subclass_of(db, other_class)
            }
        }
    }

    pub(crate) fn to_instance(self) -> Type<'db> {
        match self.subclass_of {
            ClassBase::Class(class) => Type::instance(class),
            ClassBase::Dynamic(dynamic_type) => Type::Dynamic(dynamic_type),
        }
    }
}
