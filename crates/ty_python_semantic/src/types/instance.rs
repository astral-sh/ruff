//! Instance types: both nominal and structural.

use std::borrow::Cow;
use std::marker::PhantomData;

use super::protocol_class::ProtocolInterface;
use super::{BoundTypeVarInstance, ClassType, KnownClass, SubclassOfType, Type, TypeVarVariance};
use crate::place::PlaceAndQualifiers;
use crate::semantic_index::definition::Definition;
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::enums::is_single_member_enum;
use crate::types::generics::{InferableTypeVars, walk_specialization};
use crate::types::protocol_class::{ProtocolClass, walk_protocol_interface};
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_type};
use crate::types::{
    ApplyTypeMappingVisitor, ClassBase, ClassLiteral, FindLegacyTypeVarsVisitor,
    HasRelationToVisitor, IsDisjointVisitor, IsEquivalentVisitor, NormalizedVisitor, TypeContext,
    TypeMapping, TypeRelation, VarianceInferable,
};
use crate::{Db, FxOrderSet};

pub(super) use synthesized_protocol::SynthesizedProtocolType;

impl<'db> Type<'db> {
    pub(crate) const fn object() -> Self {
        Type::NominalInstance(NominalInstanceType(NominalInstanceInner::Object))
    }

    pub(crate) const fn is_object(&self) -> bool {
        matches!(
            self,
            Type::NominalInstance(NominalInstanceType(NominalInstanceInner::Object))
        )
    }

    pub(crate) fn instance(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        let (class_literal, specialization) = class.class_literal(db);
        match class_literal.known(db) {
            Some(KnownClass::Tuple) => Type::tuple(TupleType::new(
                db,
                specialization
                    .and_then(|spec| Some(Cow::Borrowed(spec.tuple(db)?)))
                    .unwrap_or_else(|| Cow::Owned(TupleSpec::homogeneous(Type::unknown())))
                    .as_ref(),
            )),
            Some(KnownClass::Object) => Type::object(),
            _ => class_literal
                .is_typed_dict(db)
                .then(|| Type::typed_dict(class))
                .or_else(|| {
                    class.into_protocol_class(db).map(|protocol_class| {
                        Self::ProtocolInstance(ProtocolInstanceType::from_class(protocol_class))
                    })
                })
                .unwrap_or(Type::NominalInstance(NominalInstanceType(
                    NominalInstanceInner::NonTuple(class),
                ))),
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

    pub(crate) const fn is_nominal_instance(self) -> bool {
        matches!(self, Type::NominalInstance(_))
    }

    pub(crate) const fn as_nominal_instance(self) -> Option<NominalInstanceType<'db>> {
        match self {
            Type::NominalInstance(instance_type) => Some(instance_type),
            _ => None,
        }
    }

    /// Return `true` if `self` is a nominal instance of the given known class.
    pub(crate) fn is_instance_of(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        match self {
            Type::NominalInstance(instance) => instance.class(db).is_known(db, known_class),
            _ => false,
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
                &NormalizedVisitor::default(),
            ),
        ))
    }

    /// Return `true` if `self` conforms to the interface described by `protocol`.
    pub(super) fn satisfies_protocol(
        self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        let structurally_satisfied = if let Type::ProtocolInstance(self_protocol) = self {
            self_protocol.interface(db).has_relation_to_impl(
                db,
                protocol.interface(db),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            )
        } else {
            protocol
                .inner
                .interface(db)
                .members(db)
                .when_all(db, |member| {
                    member.is_satisfied_by(
                        db,
                        self,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
        };

        // Even if `self` does not satisfy the protocol from a structural perspective,
        // we may still need to consider it as satisfying the protocol if `protocol` is
        // a class-based protocol and `self` has the protocol class in its MRO.
        //
        // This matches the behaviour of other type checkers, and is required for us to
        // recognise `str` as a subtype of `Container[str]`.
        structurally_satisfied.or(db, || {
            let Some(nominal_instance) = protocol.as_nominal_type() else {
                return ConstraintSet::from(false);
            };

            // if `self` and `other` are *both* protocols, we also need to treat `self` as if it
            // were a nominal type, or we won't consider a protocol `P` that explicitly inherits
            // from a protocol `Q` to be a subtype of `Q` to be a subtype of `Q` if it overrides
            // `Q`'s members in a Liskov-incompatible way.
            let type_to_test = self
                .as_protocol_instance()
                .and_then(ProtocolInstanceType::as_nominal_type)
                .map(Type::NominalInstance)
                .unwrap_or(self);

            type_to_test.has_relation_to_impl(
                db,
                Type::NominalInstance(nominal_instance),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            )
        })
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
    match nominal.0 {
        NominalInstanceInner::ExactTuple(tuple) => {
            walk_tuple_type(db, tuple, visitor);
        }
        NominalInstanceInner::Object => {}
        NominalInstanceInner::NonTuple(class) => {
            visitor.visit_type(db, class.into());
        }
    }
}

impl<'db> NominalInstanceType<'db> {
    pub(super) fn class(&self, db: &'db dyn Db) -> ClassType<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.to_class_type(db),
            NominalInstanceInner::NonTuple(class) => class,
            NominalInstanceInner::Object => KnownClass::Object
                .try_to_class_literal(db)
                .expect("Typeshed should always have a `object` class in `builtins.pyi`")
                .default_specialization(db),
        }
    }

    pub(super) fn class_literal(&self, db: &'db dyn Db) -> ClassLiteral<'db> {
        let class = match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.to_class_type(db),
            NominalInstanceInner::NonTuple(class) => class,
            NominalInstanceInner::Object => {
                return KnownClass::Object
                    .try_to_class_literal(db)
                    .expect("Typeshed should always have a `object` class in `builtins.pyi`");
            }
        };
        let (class_literal, _) = class.class_literal(db);
        class_literal
    }

    /// Returns the [`KnownClass`] that this is a nominal instance of, or `None` if it is not an
    /// instance of a known class.
    pub(super) fn known_class(&self, db: &'db dyn Db) -> Option<KnownClass> {
        match self.0 {
            NominalInstanceInner::ExactTuple(_) => Some(KnownClass::Tuple),
            NominalInstanceInner::NonTuple(class) => class.known(db),
            NominalInstanceInner::Object => Some(KnownClass::Object),
        }
    }

    /// Returns whether this is a nominal instance of a particular [`KnownClass`].
    pub(super) fn has_known_class(&self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known_class(db) == Some(known_class)
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
            NominalInstanceInner::Object => None,
        }
    }

    /// Return `true` if this type represents instances of the class `builtins.object`.
    pub(super) const fn is_object(self) -> bool {
        matches!(self.0, NominalInstanceInner::Object)
    }

    pub(super) fn is_definition_generic(self) -> bool {
        match self.0 {
            NominalInstanceInner::NonTuple(class) => class.is_generic(),
            NominalInstanceInner::ExactTuple(_) => true,
            NominalInstanceInner::Object => false,
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
            NominalInstanceInner::NonTuple(_) | NominalInstanceInner::Object => None,
        }
    }

    /// If this is a specialized instance of `slice`, returns a [`SliceLiteral`] describing it.
    /// Otherwise returns `None`.
    ///
    /// The specialization must be one in which the typevars are solved as being statically known
    /// integers or `None`.
    pub(crate) fn slice_literal(self, db: &'db dyn Db) -> Option<SliceLiteral> {
        let class = match self.0 {
            NominalInstanceInner::ExactTuple(_) | NominalInstanceInner::Object => return None,
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
                if instance.has_known_class(db, KnownClass::NoneType) =>
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
            NominalInstanceInner::NonTuple(class) => Type::NominalInstance(NominalInstanceType(
                NominalInstanceInner::NonTuple(class.normalized_impl(db, visitor)),
            )),
            NominalInstanceInner::Object => Type::object(),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => {
                Some(Self(NominalInstanceInner::ExactTuple(
                    tuple.recursive_type_normalized_impl(db, div, nested)?,
                )))
            }
            NominalInstanceInner::NonTuple(class) => Some(Self(NominalInstanceInner::NonTuple(
                class.recursive_type_normalized_impl(db, div, nested)?,
            ))),
            NominalInstanceInner::Object => Some(Self(NominalInstanceInner::Object)),
        }
    }

    pub(super) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match (self.0, other.0) {
            (_, NominalInstanceInner::Object) => ConstraintSet::from(true),
            (
                NominalInstanceInner::ExactTuple(tuple1),
                NominalInstanceInner::ExactTuple(tuple2),
            ) => tuple1.has_relation_to_impl(
                db,
                tuple2,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),
            _ => self.class(db).has_relation_to_impl(
                db,
                other.class(db),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),
        }
    }

    pub(super) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match (self.0, other.0) {
            (
                NominalInstanceInner::ExactTuple(tuple1),
                NominalInstanceInner::ExactTuple(tuple2),
            ) => tuple1.is_equivalent_to_impl(db, tuple2, inferable, visitor),
            (NominalInstanceInner::Object, NominalInstanceInner::Object) => {
                ConstraintSet::from(true)
            }
            (NominalInstanceInner::NonTuple(class1), NominalInstanceInner::NonTuple(class2)) => {
                class1.is_equivalent_to_impl(db, class2, inferable, visitor)
            }
            _ => ConstraintSet::from(false),
        }
    }

    pub(super) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self.is_object() || other.is_object() {
            return ConstraintSet::from(false);
        }
        let mut result = ConstraintSet::from(false);
        if let Some(self_spec) = self.tuple_spec(db) {
            if let Some(other_spec) = other.tuple_spec(db) {
                let compatible = self_spec.is_disjoint_from_impl(
                    db,
                    &other_spec,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                );
                if result.union(db, compatible).is_always_satisfied(db) {
                    return result;
                }
            }
        }
        result.or(db, || {
            ConstraintSet::from(!(self.class(db)).could_coexist_in_mro_with(db, other.class(db)))
        })
    }

    pub(super) fn is_singleton(self, db: &'db dyn Db) -> bool {
        match self.0 {
            // The empty tuple is a singleton on CPython and PyPy, but not on other Python
            // implementations such as GraalPy. Its *use* as a singleton is discouraged and
            // should not be relied on for type narrowing, so we do not treat it as one.
            // See:
            // https://docs.python.org/3/reference/expressions.html#parenthesized-forms
            NominalInstanceInner::ExactTuple(_) | NominalInstanceInner::Object => false,
            NominalInstanceInner::NonTuple(class) => class
                .known(db)
                .map(KnownClass::is_singleton)
                .unwrap_or_else(|| is_single_member_enum(db, class.class_literal(db).0)),
        }
    }

    pub(super) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.is_single_valued(db),
            NominalInstanceInner::Object => false,
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
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => {
                Type::tuple(tuple.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            NominalInstanceInner::NonTuple(class) => {
                Type::NominalInstance(NominalInstanceType(NominalInstanceInner::NonTuple(
                    class.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                )))
            }
            NominalInstanceInner::Object => Type::object(),
        }
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => {
                tuple.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            NominalInstanceInner::NonTuple(class) => {
                class.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            NominalInstanceInner::Object => {}
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
    /// An instance of `object`.
    ///
    /// We model it with a dedicated enum variant since its use as "the type of all values" is so
    /// prevalent and foundational, and it's useful to be able to instantiate this without having
    /// to load the definition of `object` from the typeshed.
    Object,
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

impl<'db> VarianceInferable<'db> for NominalInstanceType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.class(db).variance_of(db, typevar)
    }
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
    if visitor.should_visit_lazy_type_attributes() {
        walk_protocol_interface(db, protocol.inner.interface(db), visitor);
    } else {
        match protocol.inner {
            Protocol::FromClass(class) => {
                if let Some(specialization) = class.class_literal(db).1 {
                    walk_specialization(db, specialization, visitor);
                }
            }
            Protocol::Synthesized(synthesized) => {
                walk_protocol_interface(db, synthesized.interface(), visitor);
            }
        }
    }
}

impl<'db> ProtocolInstanceType<'db> {
    // Keep this method private, so that the only way of constructing `ProtocolInstanceType`
    // instances is through the `Type::instance` constructor function.
    fn from_class(class: ProtocolClass<'db>) -> Self {
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

    /// If this is a class-based protocol, convert the protocol-instance into a nominal instance.
    ///
    /// If this is a synthesized protocol that does not correspond to a class definition
    /// in source code, return `None`. These are "pure" abstract types, that cannot be
    /// treated in a nominal way.
    pub(super) fn as_nominal_type(self) -> Option<NominalInstanceType<'db>> {
        match self.inner {
            Protocol::FromClass(class) => {
                Some(NominalInstanceType(NominalInstanceInner::NonTuple(*class)))
            }
            Protocol::Synthesized(_) => None,
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

    /// Return `true` if this protocol is a supertype of `object`.
    ///
    /// This indicates that the protocol represents the same set of possible runtime objects
    /// as `object` (since `object` is the universal set of *all* possible runtime objects!).
    /// Such a protocol is therefore an equivalent type to `object`, which would in fact be
    /// normalised to `object`.
    pub(super) fn is_equivalent_to_object(self, db: &'db dyn Db) -> bool {
        #[salsa::tracked(cycle_initial=initial, heap_size=ruff_memory_usage::heap_size)]
        fn is_equivalent_to_object_inner<'db>(
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
            _: (),
        ) -> bool {
            Type::object()
                .satisfies_protocol(
                    db,
                    protocol,
                    InferableTypeVars::None,
                    TypeRelation::Subtyping,
                    &HasRelationToVisitor::default(),
                    &IsDisjointVisitor::default(),
                )
                .is_always_satisfied(db)
        }

        fn initial<'db>(
            _db: &'db dyn Db,
            _id: salsa::Id,
            _value: ProtocolInstanceType<'db>,
            _: (),
        ) -> bool {
            true
        }

        is_equivalent_to_object_inner(db, self, ())
    }

    /// Return a "normalized" version of this `Protocol` type.
    ///
    /// See [`Type::normalized`] for more details.
    pub(super) fn normalized(self, db: &'db dyn Db) -> Type<'db> {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    /// Return a "normalized" version of this `Protocol` type.
    ///
    /// See [`Type::normalized`] for more details.
    pub(super) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Type<'db> {
        if self.is_equivalent_to_object(db) {
            return Type::object();
        }
        match self.inner {
            Protocol::FromClass(_) => Type::ProtocolInstance(Self::synthesized(
                SynthesizedProtocolType::new(db, self.inner.interface(db), visitor),
            )),
            Protocol::Synthesized(_) => Type::ProtocolInstance(self),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            inner: self.inner.recursive_type_normalized_impl(db, div, nested)?,
            _phantom: PhantomData,
        })
    }

    /// Return `true` if this protocol type is equivalent to the protocol `other`.
    ///
    /// TODO: consider the types of the members as well as their existence
    pub(super) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        _inferable: InferableTypeVars<'_, 'db>,
        _visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }
        let self_normalized = self.normalized(db);
        if self_normalized == Type::ProtocolInstance(other) {
            return ConstraintSet::from(true);
        }
        ConstraintSet::from(self_normalized == other.normalized(db))
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
        _inferable: InferableTypeVars<'_, 'db>,
        _visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        ConstraintSet::from(false)
    }

    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match self.inner {
            Protocol::FromClass(class) => class.instance_member(db, name),
            Protocol::Synthesized(synthesized) => synthesized.interface().instance_member(db, name),
        }
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self.inner {
            Protocol::FromClass(class) => {
                Self::from_class(class.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Protocol::Synthesized(synthesized) => Self::synthesized(
                synthesized.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
        }
    }

    pub(super) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        match self.inner {
            Protocol::FromClass(class) => {
                class.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            Protocol::Synthesized(synthesized) => {
                synthesized.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
    }

    pub(super) fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        self.inner.interface(db)
    }
}

impl<'db> VarianceInferable<'db> for ProtocolInstanceType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.inner.variance_of(db, typevar)
    }
}

/// An enumeration of the two kinds of protocol types: those that originate from a class
/// definition in source code, and those that are synthesized from a set of members.
///
/// # Ordering
///
/// Ordering between variants is stable and should be the same between runs.
/// Ordering within variants is based on the wrapped data's salsa-assigned id and not on its values.
/// The id may change between runs, or when e.g. a `Protocol` was garbage-collected and recreated.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update, PartialOrd, Ord, get_size2::GetSize,
)]
pub(super) enum Protocol<'db> {
    FromClass(ProtocolClass<'db>),
    Synthesized(SynthesizedProtocolType<'db>),
}

impl<'db> Protocol<'db> {
    /// Return the members of this protocol type
    fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        match self {
            Self::FromClass(class) => class.interface(db),
            Self::Synthesized(synthesized) => synthesized.interface(),
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::FromClass(class) => Some(Self::FromClass(
                class.recursive_type_normalized_impl(db, div, nested)?,
            )),
            Self::Synthesized(synthesized) => Some(Self::Synthesized(
                synthesized.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }
}

impl<'db> VarianceInferable<'db> for Protocol<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        match self {
            Protocol::FromClass(class_type) => class_type.variance_of(db, typevar),
            Protocol::Synthesized(synthesized_protocol_type) => {
                synthesized_protocol_type.variance_of(db, typevar)
            }
        }
    }
}

mod synthesized_protocol {
    use crate::semantic_index::definition::Definition;
    use crate::types::protocol_class::ProtocolInterface;
    use crate::types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, FindLegacyTypeVarsVisitor,
        NormalizedVisitor, Type, TypeContext, TypeMapping, TypeVarVariance, VarianceInferable,
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

        pub(super) fn apply_type_mapping_impl<'a>(
            self,
            db: &'db dyn Db,
            type_mapping: &TypeMapping<'a, 'db>,
            tcx: TypeContext<'db>,
            _visitor: &ApplyTypeMappingVisitor<'db>,
        ) -> Self {
            Self(self.0.specialized_and_normalized(db, type_mapping, tcx))
        }

        pub(super) fn find_legacy_typevars_impl(
            self,
            db: &'db dyn Db,
            binding_context: Option<Definition<'db>>,
            typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
            visitor: &FindLegacyTypeVarsVisitor<'db>,
        ) {
            self.0
                .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }

        pub(in crate::types) fn interface(self) -> ProtocolInterface<'db> {
            self.0
        }

        pub(in crate::types) fn recursive_type_normalized_impl(
            self,
            db: &'db dyn Db,
            div: Type<'db>,
            nested: bool,
        ) -> Option<Self> {
            Some(Self(
                self.0.recursive_type_normalized_impl(db, div, nested)?,
            ))
        }
    }

    impl<'db> VarianceInferable<'db> for SynthesizedProtocolType<'db> {
        fn variance_of(
            self,
            db: &'db dyn Db,
            typevar: BoundTypeVarInstance<'db>,
        ) -> TypeVarVariance {
            self.0.variance_of(db, typevar)
        }
    }
}
