//! Instance types: both nominal and structural.

use super::{ClassType, Type};
use crate::Db;

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

    pub(super) fn is_subtype_of(self, db: &'db dyn Db, other: InstanceType<'db>) -> bool {
        // N.B. The subclass relation is fully static
        self.class.is_subclass_of(db, other.class)
    }

    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: InstanceType<'db>) -> bool {
        self.class.is_equivalent_to(db, other.class)
    }

    pub(super) fn is_assignable_to(self, db: &'db dyn Db, other: InstanceType<'db>) -> bool {
        self.class.is_assignable_to(db, other.class)
    }

    pub(super) fn is_gradual_equivalent_to(
        self,
        db: &'db dyn Db,
        other: InstanceType<'db>,
    ) -> bool {
        self.class.is_gradual_equivalent_to(db, other.class)
    }
}

impl<'db> From<InstanceType<'db>> for Type<'db> {
    fn from(value: InstanceType<'db>) -> Self {
        Self::Instance(value)
    }
}

impl<'db> Type<'db> {
    pub const fn instance(class: ClassType<'db>) -> Self {
        Self::Instance(InstanceType { class })
    }
}
