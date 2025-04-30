//! Instance types: both nominal and structural.

use ruff_python_ast::name::Name;

use super::{ClassType, KnownClass, SubclassOfType, Type};
use crate::{Db, FxOrderSet};

impl<'db> Type<'db> {
    pub(crate) fn instance(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        if class.class_literal(db).0.is_protocol(db) {
            Self::ProtocolInstance(ProtocolInstanceType(Protocol::FromClass(class)))
        } else {
            Self::NominalInstance(NominalInstanceType { class })
        }
    }

    pub(crate) const fn into_nominal_instance(self) -> Option<NominalInstanceType<'db>> {
        match self {
            Type::NominalInstance(instance_type) => Some(instance_type),
            _ => None,
        }
    }

    /// Return `true` if `self` conforms to the interface described by `protocol`.
    ///
    /// TODO: we may need to split this into two methods in the future, once we start
    /// differentiating between fully-static and non-fully-static protocols.
    pub(super) fn satisfies_protocol(
        self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
    ) -> bool {
        // TODO: this should consider the types of the protocol members
        // as well as whether each member *exists* on `self`.
        protocol
            .0
            .protocol_members(db)
            .iter()
            .all(|member| !self.member(db, member).symbol.is_unbound())
    }
}

/// A type representing the set of runtime objects which are instances of a certain nominal class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update)]
pub struct NominalInstanceType<'db> {
    // Keep this field private, so that the only way of constructing `NominalInstanceType` instances
    // is through the `Type::instance` constructor function.
    class: ClassType<'db>,
}

impl<'db> NominalInstanceType<'db> {
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
        if self.class.is_final(db) && !self.class.is_subclass_of(db, other.class) {
            return true;
        }

        if other.class.is_final(db) && !other.class.is_subclass_of(db, self.class) {
            return true;
        }

        // Check to see whether the metaclasses of `self` and `other` are disjoint.
        // Avoid this check if the metaclass of either `self` or `other` is `type`,
        // however, since we end up with infinite recursion in that case due to the fact
        // that `type` is its own metaclass (and we know that `type` cannot be disjoint
        // from any metaclass, anyway).
        let type_type = KnownClass::Type.to_instance(db);
        let self_metaclass = self.class.metaclass_instance_type(db);
        if self_metaclass == type_type {
            return false;
        }
        let other_metaclass = other.class.metaclass_instance_type(db);
        if other_metaclass == type_type {
            return false;
        }
        self_metaclass.is_disjoint_from(db, other_metaclass)
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

impl<'db> From<NominalInstanceType<'db>> for Type<'db> {
    fn from(value: NominalInstanceType<'db>) -> Self {
        Self::NominalInstance(value)
    }
}

/// A `ProtocolInstanceType` represents the set of all possible runtime objects
/// that conform to the interface described by a certain protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct ProtocolInstanceType<'db>(
    // Keep the inner field here private,
    // so that the only way of constructing `ProtocolInstanceType` instances
    // is through the `Type::instance` constructor function.
    Protocol<'db>,
);

impl<'db> ProtocolInstanceType<'db> {
    pub(super) fn inner(self) -> Protocol<'db> {
        self.0
    }

    /// Return the meta-type of this protocol-instance type.
    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        match self.0 {
            Protocol::FromClass(class) => SubclassOfType::from(db, class),

            // TODO: we can and should do better here.
            //
            // This is supported by mypy, and should be supported by us as well.
            // We'll need to come up with a better solution for the meta-type of
            // synthesized protocols to solve this:
            //
            // ```py
            // from typing import Callable
            //
            // def foo(x: Callable[[], int]) -> None:
            //     reveal_type(type(x))                 # mypy: "type[def (builtins.int) -> builtins.str]"
            //     reveal_type(type(x).__call__)        # mypy: "def (*args: Any, **kwds: Any) -> Any"
            // ```
            Protocol::Synthesized(_) => KnownClass::Type.to_instance(db),
        }
    }

    /// Return a "normalized" version of this `Protocol` type.
    ///
    /// See [`Type::normalized`] for more details.
    pub(super) fn normalized(self, db: &'db dyn Db) -> Type<'db> {
        let object = KnownClass::Object.to_instance(db);
        if object.satisfies_protocol(db, self) {
            return object;
        }
        match self.0 {
            Protocol::FromClass(_) => Type::ProtocolInstance(Self(Protocol::Synthesized(
                SynthesizedProtocolType::new(db, self.0.protocol_members(db)),
            ))),
            Protocol::Synthesized(_) => Type::ProtocolInstance(self),
        }
    }

    /// TODO: this should return `true` if any of the members of this protocol type contain any `Todo` types.
    #[expect(clippy::unused_self)]
    pub(super) fn contains_todo(self) -> bool {
        false
    }

    /// Return `true` if this protocol type is fully static.
    ///
    /// TODO: should not be considered fully static if any members do not have fully static types
    #[expect(clippy::unused_self)]
    pub(super) fn is_fully_static(self) -> bool {
        true
    }

    /// Return `true` if this protocol type is a subtype of the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_subtype_of(self, db: &'db dyn Db, other: Self) -> bool {
        self.0
            .protocol_members(db)
            .is_superset(other.0.protocol_members(db))
    }

    /// Return `true` if this protocol type is assignable to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_assignable_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_subtype_of(db, other)
    }

    /// Return `true` if this protocol type is equivalent to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.normalized(db) == other.normalized(db)
    }

    /// Return `true` if this protocol type is gradually equivalent to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_equivalent_to(db, other)
    }

    /// Return `true` if this protocol type is disjoint from the protocol `other`.
    ///
    /// TODO: a protocol `X` is disjoint from a protocol `Y` if `X` and `Y`
    /// have a member with the same name but disjoint types
    #[expect(clippy::unused_self)]
    pub(super) fn is_disjoint_from(self, _db: &'db dyn Db, _other: Self) -> bool {
        false
    }
}

/// An enumeration of the two kinds of protocol types: those that originate from a class
/// definition in source code, and those that are synthesized from a set of members.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, salsa::Supertype, PartialOrd, Ord,
)]
pub(super) enum Protocol<'db> {
    FromClass(ClassType<'db>),
    Synthesized(SynthesizedProtocolType<'db>),
}

impl<'db> Protocol<'db> {
    /// Return the members of this protocol type
    fn protocol_members(self, db: &'db dyn Db) -> &'db FxOrderSet<Name> {
        match self {
            Self::FromClass(class) => class
                .class_literal(db)
                .0
                .into_protocol_class(db)
                .expect("Protocol class literal should be a protocol class")
                .protocol_members(db),
            Self::Synthesized(synthesized) => synthesized.members(db),
        }
    }
}

/// A "synthesized" protocol type that is dissociated from a class definition in source code.
///
/// Two synthesized protocol types with the same members will share the same Salsa ID,
/// making them easy to compare for equivalence. A synthesized protocol type is therefore
/// returned by [`ProtocolInstanceType::normalized`] so that two protocols with the same members
/// will be understood as equivalent even in the context of differently ordered unions or intersections.
#[salsa::interned(debug)]
pub(super) struct SynthesizedProtocolType<'db> {
    #[return_ref]
    pub(super) members: FxOrderSet<Name>,
}
