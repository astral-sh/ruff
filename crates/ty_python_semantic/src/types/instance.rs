//! Instance types: both nominal and structural.

use std::marker::PhantomData;

use super::protocol_class::ProtocolInterface;
use super::{ClassType, KnownClass, SubclassOfType, Type, TypeVarVariance};
use crate::place::{Boundness, Place, PlaceAndQualifiers};
use crate::types::{ClassLiteral, DynamicType, TypeMapping, TypeRelation, TypeVarInstance};
use crate::{Db, FxOrderSet};

pub(super) use synthesized_protocol::SynthesizedProtocolType;

impl<'db> Type<'db> {
    pub(crate) fn instance(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        if class.is_known(db, KnownClass::Any) {
            Self::Dynamic(DynamicType::Any)
        } else if class.class_literal(db).0.is_protocol(db) {
            Self::ProtocolInstance(ProtocolInstanceType::from_class(class))
        } else {
            Self::NominalInstance(NominalInstanceType::from_class(class))
        }
    }

    pub(crate) const fn into_nominal_instance(self) -> Option<NominalInstanceType<'db>> {
        match self {
            Type::NominalInstance(instance_type) => Some(instance_type),
            _ => None,
        }
    }

    pub(super) fn synthesized_protocol<'a, M>(db: &'db dyn Db, members: M) -> Self
    where
        M: IntoIterator<Item = (&'a str, Type<'db>)>,
    {
        Self::ProtocolInstance(ProtocolInstanceType::synthesized(
            SynthesizedProtocolType::new(db, ProtocolInterface::with_members(db, members)),
        ))
    }

    /// Return `true` if `self` conforms to the interface described by `protocol`.
    ///
    /// TODO: we may need to split this into two methods in the future, once we start
    /// differentiating between fully-static and non-fully-static protocols.
    pub(super) fn satisfies_protocol(
        self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
        relation: TypeRelation,
    ) -> bool {
        protocol.inner.interface(db).members(db).all(|member| {
            matches!(
                self.member(db, member.name()).place,
                Place::Type(ty, Boundness::Bound) if {
                    let is_todo_type = |ty| {
                        matches!(
                            ty,
                            // TODO: These types have some issues (recursive subtype checks, etc.), so they are not checked at the moment.
                            Type::FunctionLiteral(_)
                            | Type::BoundMethod(_)
                            | Type::PropertyInstance(_)
                        )
                    };
                    is_todo_type(ty) || is_todo_type(member.ty()) || ty.has_relation_to(db, member.ty(), relation)
                }
            )
        })
    }
}

/// A type representing the set of runtime objects which are instances of a certain nominal class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update)]
pub struct NominalInstanceType<'db> {
    pub(super) class: ClassType<'db>,

    // Keep this field private, so that the only way of constructing `NominalInstanceType` instances
    // is through the `Type::instance` constructor function.
    _phantom: PhantomData<()>,
}

impl<'db> NominalInstanceType<'db> {
    // Keep this method private, so that the only way of constructing `NominalInstanceType`
    // instances is through the `Type::instance` constructor function.
    fn from_class(class: ClassType<'db>) -> Self {
        Self {
            class,
            _phantom: PhantomData,
        }
    }

    pub(super) fn normalized(self, db: &'db dyn Db) -> Self {
        Self::from_class(self.class.normalized(db))
    }

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::from_class(self.class.materialize(db, variance))
    }

    pub(super) fn has_relation_to(
        self,
        db: &'db dyn Db,
        other: Self,
        relation: TypeRelation,
    ) -> bool {
        self.class.has_relation_to(db, other.class, relation)
    }

    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.class.is_equivalent_to(db, other.class)
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

    pub(super) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        Self::from_class(self.class.apply_type_mapping(db, type_mapping))
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        self.class.find_legacy_typevars(db, typevars);
    }
}

impl<'db> From<NominalInstanceType<'db>> for Type<'db> {
    fn from(value: NominalInstanceType<'db>) -> Self {
        Self::NominalInstance(value)
    }
}

/// A `ProtocolInstanceType` represents the set of all possible runtime objects
/// that conform to the interface described by a certain protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord)]
pub struct ProtocolInstanceType<'db> {
    pub(super) inner: Protocol<'db>,

    // Keep the inner field here private,
    // so that the only way of constructing `ProtocolInstanceType` instances
    // is through the `Type::instance` constructor function.
    _phantom: PhantomData<()>,
}

impl<'db> ProtocolInstanceType<'db> {
    // Keep this method private, so that the only way of constructing `ProtocolInstanceType`
    // instances is through the `Type::instance` constructor function.
    fn from_class(class: ClassType<'db>) -> Self {
        Self {
            inner: Protocol::FromClass(class),
            _phantom: PhantomData,
        }
    }

    // Keep this method private, so that the only way of constructing `ProtocolInstanceType`
    // instances is through the `Type::instance` constructor function.
    fn synthesized(synthesized: SynthesizedProtocolType<'db>) -> Self {
        Self {
            inner: Protocol::Synthesized(synthesized),
            _phantom: PhantomData,
        }
    }

    /// Return the meta-type of this protocol-instance type.
    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        match self.inner {
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
        if object.satisfies_protocol(db, self, TypeRelation::Subtyping) {
            return object;
        }
        match self.inner {
            Protocol::FromClass(_) => Type::ProtocolInstance(Self::synthesized(
                SynthesizedProtocolType::new(db, self.inner.interface(db)),
            )),
            Protocol::Synthesized(_) => Type::ProtocolInstance(self),
        }
    }

    /// Replace references to `class` with a self-reference marker
    pub(super) fn replace_self_reference(self, db: &'db dyn Db, class: ClassLiteral<'db>) -> Self {
        match self.inner {
            Protocol::FromClass(class_type) if class_type.class_literal(db).0 == class => {
                ProtocolInstanceType::synthesized(SynthesizedProtocolType::new(
                    db,
                    ProtocolInterface::SelfReference,
                ))
            }
            _ => self,
        }
    }

    /// Return `true` if the types of any of the members match the closure passed in.
    pub(super) fn any_over_type(
        self,
        db: &'db dyn Db,
        type_fn: &dyn Fn(Type<'db>) -> bool,
    ) -> bool {
        self.inner.interface(db).any_over_type(db, type_fn)
    }

    /// Return `true` if this protocol type is fully static.
    pub(super) fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.inner.interface(db).is_fully_static(db)
    }

    /// Return `true` if this protocol type has the given type relation to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn has_relation_to(
        self,
        db: &'db dyn Db,
        other: Self,
        relation: TypeRelation,
    ) -> bool {
        relation.applies_to(
            db,
            Type::ProtocolInstance(self),
            Type::ProtocolInstance(other),
        ) && other
            .inner
            .interface(db)
            .is_sub_interface_of(db, self.inner.interface(db))
    }

    /// Return `true` if this protocol type is equivalent to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_fully_static(db)
            && other.is_fully_static(db)
            && self.normalized(db) == other.normalized(db)
    }

    /// Return `true` if this protocol type is gradually equivalent to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.normalized(db) == other.normalized(db)
    }

    /// Return `true` if this protocol type is disjoint from the protocol `other`.
    ///
    /// TODO: a protocol `X` is disjoint from a protocol `Y` if `X` and `Y`
    /// have a member with the same name but disjoint types
    #[expect(clippy::unused_self)]
    pub(super) fn is_disjoint_from(self, _db: &'db dyn Db, _other: Self) -> bool {
        false
    }

    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match self.inner {
            Protocol::FromClass(class) => class.instance_member(db, name),
            Protocol::Synthesized(synthesized) => synthesized
                .interface()
                .member_by_name(db, name)
                .map(|member| PlaceAndQualifiers {
                    place: Place::bound(member.ty()),
                    qualifiers: member.qualifiers(),
                })
                .unwrap_or_else(|| KnownClass::Object.to_instance(db).instance_member(db, name)),
        }
    }

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self.inner {
            // TODO: This should also materialize via `class.materialize(db, variance)`
            Protocol::FromClass(class) => Self::from_class(class),
            Protocol::Synthesized(synthesized) => {
                Self::synthesized(synthesized.materialize(db, variance))
            }
        }
    }

    pub(super) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        match self.inner {
            Protocol::FromClass(class) => {
                Self::from_class(class.apply_type_mapping(db, type_mapping))
            }
            Protocol::Synthesized(synthesized) => {
                Self::synthesized(synthesized.apply_type_mapping(db, type_mapping))
            }
        }
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        match self.inner {
            Protocol::FromClass(class) => {
                class.find_legacy_typevars(db, typevars);
            }
            Protocol::Synthesized(synthesized) => {
                synthesized.find_legacy_typevars(db, typevars);
            }
        }
    }

    pub(super) fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        self.inner.interface(db)
    }
}

/// An enumeration of the two kinds of protocol types: those that originate from a class
/// definition in source code, and those that are synthesized from a set of members.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord)]
pub(super) enum Protocol<'db> {
    FromClass(ClassType<'db>),
    Synthesized(SynthesizedProtocolType<'db>),
}

impl<'db> Protocol<'db> {
    /// Return the members of this protocol type
    fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        match self {
            Self::FromClass(class) => class
                .class_literal(db)
                .0
                .into_protocol_class(db)
                .expect("Protocol class literal should be a protocol class")
                .interface(db),
            Self::Synthesized(synthesized) => synthesized.interface(),
        }
    }
}

mod synthesized_protocol {
    use crate::types::protocol_class::ProtocolInterface;
    use crate::types::{TypeMapping, TypeVarInstance, TypeVarVariance};
    use crate::{Db, FxOrderSet};

    /// A "synthesized" protocol type that is dissociated from a class definition in source code.
    ///
    /// Two synthesized protocol types with the same members will share the same Salsa ID,
    /// making them easy to compare for equivalence. A synthesized protocol type is therefore
    /// returned by [`super::ProtocolInstanceType::normalized`] so that two protocols with the same members
    /// will be understood as equivalent even in the context of differently ordered unions or intersections.
    ///
    /// The constructor method of this type maintains the invariant that a synthesized protocol type
    /// is always constructed from a *normalized* protocol interface.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord)]
    pub(in crate::types) struct SynthesizedProtocolType<'db>(ProtocolInterface<'db>);

    impl<'db> SynthesizedProtocolType<'db> {
        pub(super) fn new(db: &'db dyn Db, interface: ProtocolInterface<'db>) -> Self {
            Self(interface.normalized(db))
        }

        pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
            Self(self.0.materialize(db, variance))
        }

        pub(super) fn apply_type_mapping<'a>(
            self,
            db: &'db dyn Db,
            type_mapping: &TypeMapping<'a, 'db>,
        ) -> Self {
            Self(self.0.specialized_and_normalized(db, type_mapping))
        }

        pub(super) fn find_legacy_typevars(
            self,
            db: &'db dyn Db,
            typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
        ) {
            self.0.find_legacy_typevars(db, typevars);
        }

        pub(in crate::types) fn interface(self) -> ProtocolInterface<'db> {
            self.0
        }
    }
}
