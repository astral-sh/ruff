//! Instance types: both nominal and structural.

use super::{ClassType, KnownClass, SubclassOfType, Type};
use crate::Db;

impl<'db> Type<'db> {
    pub(crate) const fn instance(class: ClassType<'db>) -> Self {
        Self::Instance(InstanceType { class })
    }

    pub(crate) const fn into_instance(self) -> Option<InstanceType<'db>> {
        match self {
            Type::Instance(instance_type) => Some(instance_type),
            _ => None,
        }
    }
}

/// A type representing the set of runtime objects which are instances of a certain nominal class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update)]
pub struct InstanceType<'db> {
    // Keep this field private, so that the only way of constructing `InstanceType` instances
    // is through the `Type::instance` constructor function.
    class: ClassType<'db>,
}

impl<'db> InstanceType<'db> {
    pub(super) fn class(self) -> ClassType<'db> {
        self.class
    }

    pub(super) fn is_subtype_of(self, db: &'db dyn Db, other: Self) -> bool {
        // N.B. The subclass relation is fully static
        self.class.is_subclass_of(db, other.class)
    }

    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.class.is_equivalent_to(db, other.class)
    }

    pub(super) fn is_assignable_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.class.is_assignable_to(db, other.class)
    }

    pub(super) fn is_disjoint_from(self, db: &'db dyn Db, other: Self) -> bool {
        (self.class.is_final(db) && !self.class.is_subclass_of(db, other.class))
            || (other.class.is_final(db) && !other.class.is_subclass_of(db, self.class))
    }

    pub(super) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.class.is_gradual_equivalent_to(db, other.class)
    }

    pub(super) fn is_singleton(self, db: &'db dyn Db) -> bool {
        self.class.known(db).is_some_and(KnownClass::is_singleton)
    }

    pub(super) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        self.class
            .known(db)
            .is_some_and(KnownClass::is_single_valued)
    }

    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        SubclassOfType::from(db, self.class)
    }
}

impl<'db> From<InstanceType<'db>> for Type<'db> {
    fn from(value: InstanceType<'db>) -> Self {
        Self::Instance(value)
    }
}
