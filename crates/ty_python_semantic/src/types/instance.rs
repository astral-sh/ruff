//! Instance types: both nominal and structural.

use std::borrow::Cow;
use std::marker::PhantomData;

use super::protocol_class::ProtocolInterface;
use super::{BoundTypeVarInstance, ClassType, KnownClass, SubclassOfType, Type, TypeVarVariance};
use crate::place::PlaceAndQualifiers;
use crate::semantic_index::definition::Definition;
use crate::types::enums::is_single_member_enum;
use crate::types::protocol_class::walk_protocol_interface;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{
    ApplyTypeMappingVisitor, ClassBase, DynamicType, HasRelationToVisitor, IsDisjointVisitor,
    NormalizedVisitor, TypeMapping, TypeRelation, TypeTransformer,
};
use crate::{Db, FxOrderSet};

pub(super) use synthesized_protocol::SynthesizedProtocolType;

impl<'db> Type<'db> {
    pub(crate) fn instance(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        let (class_literal, specialization) = class.class_literal(db);

        match class_literal.known(db) {
            Some(KnownClass::Any) => Type::Dynamic(DynamicType::Any),
            Some(KnownClass::Tuple) => Type::tuple(TupleType::new(
                db,
                specialization
                    .and_then(|spec| Some(Cow::Borrowed(spec.tuple(db)?)))
                    .unwrap_or_else(|| Cow::Owned(TupleSpec::homogeneous(Type::unknown())))
                    .as_ref(),
            )),
            _ if class_literal.is_protocol(db) => {
                Self::ProtocolInstance(ProtocolInstanceType::from_class(class))
            }
            _ if class_literal.is_typed_dict(db) => Type::typed_dict(class),
            _ => Type::non_tuple_instance(class),
        }
    }

    pub(crate) fn tuple(tuple: Option<TupleType<'db>>) -> Self {
        let Some(tuple) = tuple else {
            return Type::Never;
        };
        Type::tuple_instance(tuple)
    }

    pub(crate) fn homogeneous_tuple(db: &'db dyn Db, element: Type<'db>) -> Self {
        Type::tuple_instance(TupleType::homogeneous(db, element))
    }

    pub(crate) fn heterogeneous_tuple<I, T>(db: &'db dyn Db, elements: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        Type::tuple(TupleType::heterogeneous(
            db,
            elements.into_iter().map(Into::into),
        ))
    }

    pub(crate) fn empty_tuple(db: &'db dyn Db) -> Self {
        Type::tuple_instance(TupleType::empty(db))
    }

    /// **Private** helper function to create a `Type::NominalInstance` from a tuple.
    fn tuple_instance(tuple: TupleType<'db>) -> Self {
        Type::NominalInstance(NominalInstanceType(NominalInstanceInner::ExactTuple(tuple)))
    }

    /// **Private** helper function to create a `Type::NominalInstance` from a class that
    /// is known not to be `Any`, a protocol class, or a typed dict class.
    fn non_tuple_instance(class: ClassType<'db>) -> Self {
        Type::NominalInstance(NominalInstanceType(NominalInstanceInner::NonTuple(class)))
    }

    pub(crate) const fn into_nominal_instance(self) -> Option<NominalInstanceType<'db>> {
        match self {
            Type::NominalInstance(instance_type) => Some(instance_type),
            _ => None,
        }
    }

    /// Synthesize a protocol instance type with a given set of read-only property members.
    pub(super) fn protocol_with_readonly_members<'a, M>(db: &'db dyn Db, members: M) -> Self
    where
        M: IntoIterator<Item = (&'a str, Type<'db>)>,
    {
        Self::ProtocolInstance(ProtocolInstanceType::synthesized(
            SynthesizedProtocolType::new(
                db,
                ProtocolInterface::with_property_members(db, members),
                &TypeTransformer::default(),
            ),
        ))
    }

    /// Return `true` if `self` conforms to the interface described by `protocol`.
    pub(super) fn satisfies_protocol(
        self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
        relation: TypeRelation,
        visitor: &HasRelationToVisitor<'db>,
    ) -> bool {
        protocol
            .inner
            .interface(db)
            .members(db)
            .all(|member| member.is_satisfied_by(db, self, relation, visitor))
    }
}

/// A type representing the set of runtime objects which are instances of a certain nominal class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, get_size2::GetSize)]
pub struct NominalInstanceType<'db>(
    // Keep this field private, so that the only way of constructing `NominalInstanceType` instances
    // is through the `Type::instance` constructor function.
    NominalInstanceInner<'db>,
);

pub(super) fn walk_nominal_instance_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    nominal: NominalInstanceType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, nominal.class(db).into());
}

impl<'db> NominalInstanceType<'db> {
    pub(super) fn class(&self, db: &'db dyn Db) -> ClassType<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.to_class_type(db),
            NominalInstanceInner::NonTuple(class) => class,
        }
    }

    /// If this is an instance type where the class has a tuple spec, returns the tuple spec.
    ///
    /// I.e., for the type `tuple[int, str]`, this will return the tuple spec `[int, str]`.
    /// For a subclass of `tuple[int, str]`, it will return the same tuple spec.
    pub(super) fn tuple_spec(&self, db: &'db dyn Db) -> Option<Cow<'db, TupleSpec<'db>>> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => Some(Cow::Borrowed(tuple.tuple(db))),
            NominalInstanceInner::NonTuple(class) => {
                // Avoid an expensive MRO traversal for common stdlib classes.
                if class
                    .known(db)
                    .is_some_and(|known_class| !known_class.is_tuple_subclass())
                {
                    return None;
                }
                class
                    .iter_mro(db)
                    .filter_map(ClassBase::into_class)
                    .find_map(|class| match class.known(db)? {
                        // N.B. this is a pure optimisation: iterating through the MRO would give us
                        // the correct tuple spec for `sys._version_info`, since we special-case the class
                        // in `ClassLiteral::explicit_bases()` so that it is inferred as inheriting from
                        // a tuple type with the correct spec for the user's configured Python version and platform.
                        KnownClass::VersionInfo => {
                            Some(Cow::Owned(TupleSpec::version_info_spec(db)))
                        }
                        KnownClass::Tuple => Some(
                            class
                                .into_generic_alias()
                                .and_then(|alias| {
                                    Some(Cow::Borrowed(alias.specialization(db).tuple(db)?))
                                })
                                .unwrap_or_else(|| {
                                    Cow::Owned(TupleSpec::homogeneous(Type::unknown()))
                                }),
                        ),
                        _ => None,
                    })
            }
        }
    }

    /// Return `true` if this type represents instances of the class `builtins.object`.
    pub(super) fn is_object(self, db: &'db dyn Db) -> bool {
        match self.0 {
            NominalInstanceInner::ExactTuple(_) => false,
            NominalInstanceInner::NonTuple(class) => class.is_object(db),
        }
    }

    /// If this type is an *exact* tuple type (*not* a subclass of `tuple`), returns the
    /// tuple spec.
    ///
    /// You usually don't want to use this method, as you usually want to consider a subclass
    /// of a tuple type in the same way as the `tuple` type itself. Only use this method if you
    /// are certain that a *literal tuple* is required, and that a subclass of tuple will not
    /// do.
    ///
    /// I.e., for the type `tuple[int, str]`, this will return the tuple spec `[int, str]`.
    /// But for a subclass of `tuple[int, str]`, it will return `None`.
    pub(super) fn own_tuple_spec(&self, db: &'db dyn Db) -> Option<Cow<'db, TupleSpec<'db>>> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => Some(Cow::Borrowed(tuple.tuple(db))),
            NominalInstanceInner::NonTuple(_) => None,
        }
    }

    /// If this is a specialized instance of `slice`, returns a [`SliceLiteral`] describing it.
    /// Otherwise returns `None`.
    ///
    /// The specialization must be one in which the typevars are solved as being statically known
    /// integers or `None`.
    pub(crate) fn slice_literal(self, db: &'db dyn Db) -> Option<SliceLiteral> {
        let class = match self.0 {
            NominalInstanceInner::ExactTuple(_) => return None,
            NominalInstanceInner::NonTuple(class) => class,
        };
        let (class, Some(specialization)) = class.class_literal(db) else {
            return None;
        };
        if !class.is_known(db, KnownClass::Slice) {
            return None;
        }
        let [start, stop, step] = specialization.types(db) else {
            return None;
        };

        let to_u32 = |ty: &Type<'db>| match ty {
            Type::IntLiteral(n) => i32::try_from(*n).map(Some).ok(),
            Type::BooleanLiteral(b) => Some(Some(i32::from(*b))),
            Type::NominalInstance(instance)
                if instance.class(db).is_known(db, KnownClass::NoneType) =>
            {
                Some(None)
            }
            _ => None,
        };
        Some(SliceLiteral {
            start: to_u32(start)?,
            stop: to_u32(stop)?,
            step: to_u32(step)?,
        })
    }

    pub(super) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Type<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => {
                Type::tuple(tuple.normalized_impl(db, visitor))
            }
            NominalInstanceInner::NonTuple(class) => {
                Type::non_tuple_instance(class.normalized_impl(db, visitor))
            }
        }
    }

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Type<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => Type::tuple(tuple.materialize(db, variance)),
            NominalInstanceInner::NonTuple(class) => {
                Type::non_tuple_instance(class.materialize(db, variance))
            }
        }
    }

    pub(super) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        relation: TypeRelation,
        visitor: &HasRelationToVisitor<'db>,
    ) -> bool {
        match (self.0, other.0) {
            (
                NominalInstanceInner::ExactTuple(tuple1),
                NominalInstanceInner::ExactTuple(tuple2),
            ) => tuple1.has_relation_to_impl(db, tuple2, relation, visitor),
            _ => self
                .class(db)
                .has_relation_to_impl(db, other.class(db), relation, visitor),
        }
    }

    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        match (self.0, other.0) {
            (
                NominalInstanceInner::ExactTuple(tuple1),
                NominalInstanceInner::ExactTuple(tuple2),
            ) => tuple1.is_equivalent_to(db, tuple2),
            (NominalInstanceInner::NonTuple(class1), NominalInstanceInner::NonTuple(class2)) => {
                class1.is_equivalent_to(db, class2)
            }
            _ => false,
        }
    }

    pub(super) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        visitor: &IsDisjointVisitor<'db>,
    ) -> bool {
        if let Some(self_spec) = self.tuple_spec(db) {
            if let Some(other_spec) = other.tuple_spec(db) {
                if self_spec.is_disjoint_from_impl(db, &other_spec, visitor) {
                    return true;
                }
            }
        }
        !self
            .class(db)
            .could_coexist_in_mro_with(db, other.class(db))
    }

    pub(super) fn is_singleton(self, db: &'db dyn Db) -> bool {
        match self.0 {
            // The empty tuple is a singleton on CPython and PyPy, but not on other Python
            // implementations such as GraalPy. Its *use* as a singleton is discouraged and
            // should not be relied on for type narrowing, so we do not treat it as one.
            // See:
            // https://docs.python.org/3/reference/expressions.html#parenthesized-forms
            NominalInstanceInner::ExactTuple(_) => false,
            NominalInstanceInner::NonTuple(class) => class
                .known(db)
                .map(KnownClass::is_singleton)
                .unwrap_or_else(|| is_single_member_enum(db, class.class_literal(db).0)),
        }
    }

    pub(super) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.is_single_valued(db),
            NominalInstanceInner::NonTuple(class) => class
                .known(db)
                .and_then(KnownClass::is_single_valued)
                .or_else(|| Some(self.tuple_spec(db)?.is_single_valued(db)))
                .unwrap_or_else(|| is_single_member_enum(db, class.class_literal(db).0)),
        }
    }

    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        SubclassOfType::from(db, self.class(db))
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => {
                Type::tuple(tuple.apply_type_mapping_impl(db, type_mapping, visitor))
            }
            NominalInstanceInner::NonTuple(class) => {
                Type::non_tuple_instance(class.apply_type_mapping_impl(db, type_mapping, visitor))
            }
        }
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => {
                tuple.find_legacy_typevars(db, binding_context, typevars);
            }
            NominalInstanceInner::NonTuple(class) => {
                class.find_legacy_typevars(db, binding_context, typevars);
            }
        }
    }
}

impl<'db> From<NominalInstanceType<'db>> for Type<'db> {
    fn from(value: NominalInstanceType<'db>) -> Self {
        Self::NominalInstance(value)
    }
}

/// [`NominalInstanceType`] is split into two variants internally as a pure
/// optimization to avoid having to materialize the [`ClassType`] for tuple
/// instances where it would be unnecessary (this is somewhat expensive!).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, salsa::Update, get_size2::GetSize)]
enum NominalInstanceInner<'db> {
    /// A tuple type, e.g. `tuple[int, str]`.
    ///
    /// Note that the type `tuple[int, str]` includes subtypes of `tuple[int, str]`,
    /// but those subtypes would be represented using the `NonTuple` variant.
    ExactTuple(TupleType<'db>),
    /// Any instance type that does not represent some kind of instance of the
    /// builtin `tuple` class.
    ///
    /// This variant includes types that are subtypes of "exact tuple" types,
    /// because they represent "all instances of a class that is a tuple subclass".
    NonTuple(ClassType<'db>),
}

pub(crate) struct SliceLiteral {
    pub(crate) start: Option<i32>,
    pub(crate) stop: Option<i32>,
    pub(crate) step: Option<i32>,
}

/// A `ProtocolInstanceType` represents the set of all possible runtime objects
/// that conform to the interface described by a certain protocol.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord, get_size2::GetSize,
)]
pub struct ProtocolInstanceType<'db> {
    pub(super) inner: Protocol<'db>,

    // Keep the inner field here private,
    // so that the only way of constructing `ProtocolInstanceType` instances
    // is through the `Type::instance` constructor function.
    _phantom: PhantomData<()>,
}

pub(super) fn walk_protocol_instance_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    protocol: ProtocolInstanceType<'db>,
    visitor: &V,
) {
    walk_protocol_interface(db, protocol.inner.interface(db), visitor);
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
        self.normalized_impl(db, &TypeTransformer::default())
    }

    /// Return a "normalized" version of this `Protocol` type.
    ///
    /// See [`Type::normalized`] for more details.
    pub(super) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Type<'db> {
        let object = Type::object(db);
        if object.satisfies_protocol(
            db,
            self,
            TypeRelation::Subtyping,
            &HasRelationToVisitor::new(true),
        ) {
            return object;
        }
        match self.inner {
            Protocol::FromClass(_) => Type::ProtocolInstance(Self::synthesized(
                SynthesizedProtocolType::new(db, self.inner.interface(db), visitor),
            )),
            Protocol::Synthesized(_) => Type::ProtocolInstance(self),
        }
    }

    /// Return `true` if this protocol type has the given type relation to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn has_relation_to(
        self,
        db: &'db dyn Db,
        other: Self,
        _relation: TypeRelation,
    ) -> bool {
        other
            .inner
            .interface(db)
            .is_sub_interface_of(db, self.inner.interface(db))
    }

    /// Return `true` if this protocol type is equivalent to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }
        let self_normalized = self.normalized(db);
        if self_normalized == Type::ProtocolInstance(other) {
            return true;
        }
        self_normalized == other.normalized(db)
    }

    /// Return `true` if this protocol type is disjoint from the protocol `other`.
    ///
    /// TODO: a protocol `X` is disjoint from a protocol `Y` if `X` and `Y`
    /// have a member with the same name but disjoint types
    #[expect(clippy::unused_self)]
    pub(super) fn is_disjoint_from_impl(
        self,
        _db: &'db dyn Db,
        _other: Self,
        _visitor: &IsDisjointVisitor<'db>,
    ) -> bool {
        false
    }

    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match self.inner {
            Protocol::FromClass(class) => class.instance_member(db, name),
            Protocol::Synthesized(synthesized) => synthesized.interface().instance_member(db, name),
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

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self.inner {
            Protocol::FromClass(class) => {
                Self::from_class(class.apply_type_mapping_impl(db, type_mapping, visitor))
            }
            Protocol::Synthesized(synthesized) => {
                Self::synthesized(synthesized.apply_type_mapping_impl(db, type_mapping, visitor))
            }
        }
    }

    pub(super) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        match self.inner {
            Protocol::FromClass(class) => {
                class.find_legacy_typevars(db, binding_context, typevars);
            }
            Protocol::Synthesized(synthesized) => {
                synthesized.find_legacy_typevars(db, binding_context, typevars);
            }
        }
    }

    pub(super) fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        self.inner.interface(db)
    }
}

/// An enumeration of the two kinds of protocol types: those that originate from a class
/// definition in source code, and those that are synthesized from a set of members.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord, get_size2::GetSize,
)]
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
    use crate::semantic_index::definition::Definition;
    use crate::types::protocol_class::ProtocolInterface;
    use crate::types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, NormalizedVisitor, TypeMapping,
        TypeVarVariance,
    };
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
    #[derive(
        Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord, get_size2::GetSize,
    )]
    pub(in crate::types) struct SynthesizedProtocolType<'db>(ProtocolInterface<'db>);

    impl<'db> SynthesizedProtocolType<'db> {
        pub(super) fn new(
            db: &'db dyn Db,
            interface: ProtocolInterface<'db>,
            visitor: &NormalizedVisitor<'db>,
        ) -> Self {
            Self(interface.normalized_impl(db, visitor))
        }

        pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
            Self(self.0.materialize(db, variance))
        }

        pub(super) fn apply_type_mapping_impl<'a>(
            self,
            db: &'db dyn Db,
            type_mapping: &TypeMapping<'a, 'db>,
            _visitor: &ApplyTypeMappingVisitor<'db>,
        ) -> Self {
            Self(self.0.specialized_and_normalized(db, type_mapping))
        }

        pub(super) fn find_legacy_typevars(
            self,
            db: &'db dyn Db,
            binding_context: Option<Definition<'db>>,
            typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        ) {
            self.0.find_legacy_typevars(db, binding_context, typevars);
        }

        pub(in crate::types) fn interface(self) -> ProtocolInterface<'db> {
            self.0
        }
    }
}
