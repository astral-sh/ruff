//! Instance types: both nominal and structural.

use std::borrow::Cow;
use std::cell::Cell;
use std::marker::PhantomData;

use ruff_python_ast::name::Name;
use ty_module_resolver::{ModuleName, file_to_module};

use super::protocol_class::ProtocolInterface;
use super::{
    BoundTypeVarIdentity, BoundTypeVarInstance, ClassType, DivergentType, KnownClass,
    MaterializationKind, SubclassOfType, Type, TypeAliasType, TypeVarVariance,
};
use crate::place::PlaceAndQualifiers;
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension,
};
use crate::types::enums::is_single_member_enum;
use crate::types::generics::{InferableTypeVars, walk_specialization};
use crate::types::protocol_class::{
    ProtocolClass, has_all_protocol_members_defined, walk_protocol_interface,
    walk_protocol_interface_requirements,
};
use crate::types::relation::{
    DisjointnessChecker, HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeRelationChecker,
};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_type};
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, ClassBase, ClassLiteral, ErrorContext,
    FindLegacyTypeVarsVisitor, LiteralValueTypeKind, TypeContext, TypeMapping, VarianceInferable,
};
use crate::{Db, FxOrderSet};
use ty_python_core::definition::Definition;

use self::interface_protocol::ProtocolInterfaceType;

impl<'db> Type<'db> {
    pub(crate) const fn object() -> Self {
        Type::NominalInstance(NominalInstanceType(NominalInstanceInner::Object))
    }

    pub(crate) const fn is_object(&self) -> bool {
        matches!(
            self,
            Type::NominalInstance(NominalInstanceType(NominalInstanceInner::Object))
                | Type::Divergent(DivergentType {
                    materialization: Some(MaterializationKind::Top),
                    ..
                })
        )
    }

    pub(crate) fn instance(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        match class.class_literal(db) {
            // Dynamic classes created via `type()` don't have special instance types.
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicEnum(_) => {
                Type::NominalInstance(NominalInstanceType::from_class(db, class))
            }
            // Functional TypedDicts return a TypedDict instance type.
            ClassLiteral::DynamicTypedDict(_) => Type::typed_dict(class),
            ClassLiteral::Static(class_literal) => {
                let specialization = class.into_generic_alias().map(|g| g.specialization(db));
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
                                Self::ProtocolInstance(ProtocolInstanceType::from_class(
                                    protocol_class,
                                ))
                            })
                        })
                        .unwrap_or_else(|| {
                            Type::NominalInstance(NominalInstanceType::from_class(db, class))
                        }),
                }
            }
        }
    }

    pub(crate) fn tuple(tuple: Option<TupleType<'db>>) -> Self {
        let Some(tuple) = tuple else {
            return Type::Never;
        };
        Type::tuple_instance(tuple)
    }

    pub fn homogeneous_tuple(db: &'db dyn Db, element: Type<'db>) -> Self {
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

    pub(crate) const fn sys_version_info() -> Self {
        // Keep construction query-free: resolving the backing typeshed class here is on the hot
        // path for projects with many version guards. Resolve it lazily when class behavior is
        // actually needed instead.
        Type::NominalInstance(NominalInstanceType(NominalInstanceInner::SysVersionInfo))
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
            db,
            ProtocolInterface::with_property_members(db, members),
        ))
    }

    /// Synthesize a protocol instance type with a given set of methods.
    pub(super) fn protocol_with_methods<'a, M>(db: &'db dyn Db, methods: M) -> Self
    where
        M: IntoIterator<Item = (&'a str, CallableType<'db>)>,
    {
        Self::ProtocolInstance(ProtocolInstanceType::synthesized(
            db,
            ProtocolInterface::with_methods(db, methods),
        ))
    }
}

/// A type representing the set of runtime objects which are instances of a certain nominal class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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
        NominalInstanceInner::NonTuple(class) => visitor.visit_type(db, class.class(db).into()),
        NominalInstanceInner::SysVersionInfo => {}
    }
}

impl<'db> NominalInstanceType<'db> {
    fn from_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        Self(NominalInstanceInner::NonTuple(
            NominalInstanceClass::from_class(db, class),
        ))
    }

    /// Return whether this instance's class inherits from an explicit `Any` base.
    pub(super) const fn inherits_from_explicit_any(self) -> bool {
        match self.0 {
            NominalInstanceInner::NonTuple(class) => class.inherits_from_explicit_any(),
            _ => false,
        }
    }

    /// Returns the name of the class this is an instance of.
    ///
    /// For example, for an instance of `builtins.str`, this returns `"str"`.
    ///
    /// As of 2026-02-16, this method is not used in any crates in the Ruff
    /// repo, but is exposed as a public API for external users of
    /// `ty_python_semantic`.
    pub fn class_name(&self, db: &'db dyn Db) -> &'db Name {
        self.class(db).name(db)
    }

    /// Returns the fully qualified module name of the module in which the class
    /// is defined, if it can be resolved.
    ///
    /// For example, for an instance of `pathlib.Path`, this returns
    /// `Some("pathlib")`. Returns `None` if the class's file cannot be resolved
    /// to a known module (e.g. for classes defined in scripts or notebooks).
    ///
    /// As of 2026-02-16, this method is not used in any crates in the Ruff
    /// repo, but is exposed as a public API for external users of
    /// `ty_python_semantic`.
    pub fn class_module_name(&self, db: &'db dyn Db) -> Option<&'db ModuleName> {
        let file = self.class(db).class_literal(db).file(db);
        file_to_module(db, file).map(|module| module.name(db))
    }

    pub(super) fn class(&self, db: &'db dyn Db) -> ClassType<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.to_class_type(db),
            NominalInstanceInner::NonTuple(class) => class.class(db),
            NominalInstanceInner::SysVersionInfo => {
                sys_version_info_class(db).unwrap_or_else(|| ClassType::object(db))
            }
            NominalInstanceInner::Object => ClassType::object(db),
        }
    }

    /// Returns the class literal for this instance.
    pub(super) fn class_literal(&self, db: &'db dyn Db) -> ClassLiteral<'db> {
        self.class(db).class_literal(db)
    }

    /// Returns the [`KnownClass`] that this is a nominal instance of, or `None` if it is not an
    /// instance of a known class.
    pub(super) fn known_class(&self, db: &'db dyn Db) -> Option<KnownClass> {
        match self.0 {
            NominalInstanceInner::ExactTuple(_) => Some(KnownClass::Tuple),
            NominalInstanceInner::NonTuple(class) => class.class(db).known(db),
            NominalInstanceInner::SysVersionInfo => Some(KnownClass::VersionInfo),
            NominalInstanceInner::Object => Some(KnownClass::Object),
        }
    }

    pub(super) const fn is_sys_version_info(self) -> bool {
        matches!(self.0, NominalInstanceInner::SysVersionInfo)
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
            NominalInstanceInner::SysVersionInfo => {
                Some(Cow::Owned(TupleSpec::version_info_spec(db)))
            }
            NominalInstanceInner::Object => None,
            NominalInstanceInner::NonTuple(class) => {
                let class = class.class(db);
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
    pub(super) const fn is_object(self) -> bool {
        matches!(self.0, NominalInstanceInner::Object)
    }

    pub(super) fn is_definition_generic(self, db: &'db dyn Db) -> bool {
        match self.0 {
            NominalInstanceInner::ExactTuple(_) => true,
            NominalInstanceInner::SysVersionInfo | NominalInstanceInner::Object => false,
            NominalInstanceInner::NonTuple(class) => class.class(db).is_generic(),
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
            NominalInstanceInner::NonTuple(_)
            | NominalInstanceInner::SysVersionInfo
            | NominalInstanceInner::Object => None,
        }
    }

    /// If this is a specialized instance of `slice`, returns a [`SliceLiteral`] describing it.
    /// Otherwise returns `None`.
    ///
    /// The specialization must be one in which the typevars are solved as being statically known
    /// integers or `None`.
    pub(crate) fn slice_literal(self, db: &'db dyn Db) -> Option<SliceLiteral> {
        let class = match self.0 {
            NominalInstanceInner::NonTuple(class) => class.class(db),
            NominalInstanceInner::ExactTuple(_)
            | NominalInstanceInner::SysVersionInfo
            | NominalInstanceInner::Object => return None,
        };
        let (class_literal, specialization) = class.static_class_literal(db)?;
        let specialization = specialization?;
        if !class_literal.is_known(db, KnownClass::Slice) {
            return None;
        }
        let [start, stop, step] = specialization.types(db) else {
            return None;
        };

        let to_u32 = |ty: &Type<'db>| match ty {
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Int(n) => i32::try_from(n.as_i64()).map(Some).ok(),
                LiteralValueTypeKind::Bool(b) => Some(Some(i32::from(b))),
                _ => None,
            },
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
            NominalInstanceInner::SysVersionInfo => {
                Some(Self(NominalInstanceInner::SysVersionInfo))
            }
            NominalInstanceInner::Object => Some(Self(NominalInstanceInner::Object)),
            NominalInstanceInner::NonTuple(class) => {
                let transformed = class
                    .class(db)
                    .recursive_type_normalized_impl(db, div, nested)?;
                Some(Self(NominalInstanceInner::NonTuple(
                    class.with_class(db, transformed),
                )))
            }
        }
    }

    pub(super) fn is_singleton(self, db: &'db dyn Db) -> bool {
        match self.0 {
            // The empty tuple is a singleton on CPython and PyPy, but not on other Python
            // implementations such as GraalPy. Its *use* as a singleton is discouraged and
            // should not be relied on for type narrowing, so we do not treat it as one.
            // See:
            // https://docs.python.org/3/reference/expressions.html#parenthesized-forms
            NominalInstanceInner::ExactTuple(_) | NominalInstanceInner::Object => false,
            NominalInstanceInner::SysVersionInfo => true,
            NominalInstanceInner::NonTuple(class) => class
                .class(db)
                .known(db)
                .map(KnownClass::is_singleton)
                .unwrap_or_else(|| is_single_member_enum(db, class.class(db).class_literal(db))),
        }
    }

    pub(super) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => tuple.is_single_valued(db),
            NominalInstanceInner::Object => false,
            NominalInstanceInner::SysVersionInfo => true,
            NominalInstanceInner::NonTuple(class) => class
                .class(db)
                .known(db)
                .and_then(KnownClass::is_single_valued)
                .or_else(|| Some(self.tuple_spec(db)?.is_single_valued(db)))
                .unwrap_or_else(|| is_single_member_enum(db, class.class(db).class_literal(db))),
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
            NominalInstanceInner::SysVersionInfo => Type::NominalInstance(self),
            NominalInstanceInner::Object => Type::object(),
            NominalInstanceInner::NonTuple(class) => {
                let transformed =
                    class
                        .class(db)
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                Type::NominalInstance(Self(NominalInstanceInner::NonTuple(
                    class.with_class(db, transformed),
                )))
            }
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
            NominalInstanceInner::SysVersionInfo | NominalInstanceInner::Object => {}
            NominalInstanceInner::NonTuple(class) => {
                class
                    .class(db)
                    .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
    }
}

impl<'db> From<NominalInstanceType<'db>> for Type<'db> {
    fn from(value: NominalInstanceType<'db>) -> Self {
        Self::NominalInstance(value)
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Return `true` if `ty` conforms to the interface described by `protocol`.
    pub(super) fn check_type_satisfies_protocol(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // `ty` might satisfy the protocol nominally, if `protocol` is a class-based protocol and
        // `ty` has the protocol class in its MRO. This is a much cheaper check than the
        // structural check we perform below, so we do it first to avoid the structural check when
        // we can.
        let mut result = self.never();

        let source_protocol = ty.as_protocol_instance();
        let materialized_source_changes_target = source_protocol.is_some_and(|source| {
            source.materialization_changes_requirements(db, protocol.interface(db))
        });

        if !protocol.is_materialized(db)
            && !materialized_source_changes_target
            && let Some(nominal_instance) = protocol.to_nominal_instance(db)
        {
            // if `ty` and `protocol` are *both* protocols, we also need to treat `ty` as if it
            // were a nominal type, or we won't consider a protocol `P` that explicitly inherits
            // from a protocol `Q` to be a subtype of `Q` to be a subtype of `Q` if it overrides
            // `Q`'s members in a Liskov-incompatible way.
            let type_to_test = source_protocol
                .and_then(|protocol| protocol.to_nominal_instance(db))
                .map(Type::NominalInstance)
                .unwrap_or(ty);

            let nominally_satisfied =
                self.check_type_pair(db, type_to_test, Type::NominalInstance(nominal_instance));

            if result
                .union(db, self.constraints, nominally_satisfied)
                .is_always_satisfied(db)
            {
                return result;
            }

            // For union simplification, failing the nominal relation between two
            // specializations of the same protocol class is enough to keep both union elements.
            // Falling back to the structural relation can recursively compare every protocol
            // member even though a failed redundancy check only means that we preserve a
            // potentially redundant union arm.
            if matches!(self.relation, TypeRelation::Redundancy { pure: false })
                && ty
                    .as_protocol_instance()
                    .and_then(ProtocolInstanceType::to_nominal_instance)
                    .is_some_and(|source_instance| {
                        source_instance.class(db).class_literal(db)
                            == nominal_instance.class(db).class_literal(db)
                    })
            {
                return nominally_satisfied;
            }
        }

        // `Generator` special case: compare the type parameters nominally. Prior to 3.13, its
        // return type does not appear non-recursively in the protocol; from 3.13 onward,
        // structurally inferring through `close() -> ReturnT | None` can spuriously infer `None`.
        // TODO: Remove the Python 3.13+ extension of this special case once
        // https://github.com/astral-sh/ty/issues/3596 is fixed.
        if let Some(source_protocol) = ty.as_protocol_instance()
            && let Some(source_class) = source_protocol.class_origin(db)
            && let Some(proto_class) = protocol.class_origin(db)
            && source_class.is_known(db, KnownClass::Generator)
            && proto_class.is_known(db, KnownClass::Generator)
        {
            return result;
        }

        // Fast path: skip expensive per-member type comparisons when members are plainly
        // missing. When collecting error context, we continue and let the structural check
        // below report per-member errors instead.
        if !self.is_context_collection_enabled()
            && !has_all_protocol_members_defined(db, ty, protocol)
        {
            return result;
        }

        let structurally_satisfied = if let Type::ProtocolInstance(source_protocol) = ty {
            self.check_protocol_interface_pair(
                db,
                ty,
                source_protocol.interface(db),
                protocol.interface(db),
            )
        } else {
            protocol
                .inner
                .interface(db)
                .members(db)
                .when_all(db, self.constraints, |member| {
                    self.type_satisfies_protocol_member(db, ty, &member)
                })
        };
        if let Some(context) = self.report_context()
            && structurally_satisfied.is_never_satisfied(db)
        {
            context.push(ErrorContext::TypeNotCompatibleWithProtocol {
                ty,
                protocol: Type::ProtocolInstance(protocol),
            });
        }
        result.or(db, self.constraints, || structurally_satisfied)
    }

    /// Return whether a class-object type inhabits `type[protocol]`.
    ///
    /// The effective constructor return must satisfy the instance protocol, while the class object
    /// itself must provide the protocol's `ClassVar` and unbound method requirements. Ordinary
    /// instance attributes and properties are intentionally not required on the class object.
    ///
    /// `meta_ty` must be a class-object type represented by `ClassLiteral`, `SubclassOf`, or
    /// `GenericAlias`. Other types are not necessarily subtypes of `type` or callable, and could
    /// therefore incorrectly satisfy this check through an `Unknown` constructor return type.
    pub(super) fn check_meta_type_satisfies_protocol(
        &self,
        db: &'db dyn Db,
        meta_ty: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        debug_assert!(matches!(
            meta_ty,
            Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_)
        ));

        let constructed_ty = meta_ty.bindings(db).return_type(db);
        self.check_type_pair(db, constructed_ty, Type::ProtocolInstance(protocol))
            .and(db, self.constraints, || {
                self.check_meta_protocol_members(db, constructed_ty, meta_ty, protocol)
            })
    }

    pub(super) fn check_nominal_instance_pair(
        &self,
        db: &'db dyn Db,
        source: NominalInstanceType<'db>,
        target: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match (source.0, target.0) {
            (_, NominalInstanceInner::Object) => self.always(),
            (
                NominalInstanceInner::ExactTuple(source_tuple),
                NominalInstanceInner::ExactTuple(target_tuple),
            ) => self.check_tuple_type_pair(db, source_tuple, target_tuple),
            _ => self.check_class_pair(db, source.class(db), target.class(db)),
        }
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    /// Return `true` if this protocol type is disjoint from the protocol `other`.
    ///
    /// TODO: a protocol `X` is disjoint from a protocol `Y` if `X` and `Y`
    /// have a member with the same name but disjoint types
    pub(super) fn check_protocol_instance_pair(
        &self,
        _db: &'db dyn Db,
        _left: ProtocolInstanceType<'db>,
        _right: ProtocolInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.never()
    }

    pub(super) fn check_nominal_instance_pair(
        &self,
        db: &'db dyn Db,
        left: NominalInstanceType<'db>,
        right: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let mut result = self.never();
        if left.is_object() || right.is_object() {
            return result;
        }
        if let Some(left_spec) = left.tuple_spec(db) {
            if let Some(right_spec) = right.tuple_spec(db) {
                let compatible = self.check_tuple_spec_pair(db, &left_spec, &right_spec);
                if result
                    .union(db, self.constraints, compatible)
                    .is_always_satisfied(db)
                {
                    return result;
                }
            }
        }

        result.or(db, self.constraints, || {
            ConstraintSet::from_bool(
                self.constraints,
                !left
                    .class(db)
                    .could_coexist_in_mro_with_disjointness_checker(db, right.class(db), self),
            )
        })
    }
}

/// The class of a nominal instance whose MRO contains an explicit `Any` base.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ExplicitAnyInstanceClass<'db> {
    #[returns(copy)]
    class: ClassType<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ExplicitAnyInstanceClass<'_> {}

/// The class stored by a non-tuple nominal instance.
///
/// Interning the uncommon explicit-`Any` case lets this type store the additional semantic bit
/// without increasing the size of [`Type`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum NominalInstanceClass<'db> {
    Plain(ClassType<'db>),
    InheritsFromExplicitAny(ExplicitAnyInstanceClass<'db>),
}

impl<'db> NominalInstanceClass<'db> {
    fn from_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        if class.class_literal(db).inherits_from_explicit_any(db) {
            Self::InheritsFromExplicitAny(ExplicitAnyInstanceClass::new(db, class))
        } else {
            Self::Plain(class)
        }
    }

    const fn inherits_from_explicit_any(self) -> bool {
        matches!(self, Self::InheritsFromExplicitAny(_))
    }

    fn class(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::Plain(class) => class,
            Self::InheritsFromExplicitAny(class) => class.class(db),
        }
    }

    fn with_class(self, db: &'db dyn Db, class: ClassType<'db>) -> Self {
        match self {
            Self::Plain(_) => Self::Plain(class),
            Self::InheritsFromExplicitAny(_) => {
                Self::InheritsFromExplicitAny(ExplicitAnyInstanceClass::new(db, class))
            }
        }
    }
}

/// [`NominalInstanceType`] is split into several variants internally as a pure optimization to
/// avoid having to materialize the [`ClassType`] for tuple instances where it would be unnecessary
/// (this is somewhat expensive!).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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
    NonTuple(NominalInstanceClass<'db>),
    /// The singleton `sys.version_info` value.
    SysVersionInfo,
}

fn sys_version_info_class(db: &dyn Db) -> Option<ClassType<'_>> {
    KnownClass::VersionInfo
        .try_to_class_literal(db)
        .map(|class| class.default_specialization(db))
}

pub(crate) struct SliceLiteral {
    pub(crate) start: Option<i32>,
    pub(crate) stop: Option<i32>,
    pub(crate) step: Option<i32>,
}

impl<'db> VarianceInferable<'db> for NominalInstanceType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarIdentity<'db>) -> TypeVarVariance {
        self.class(db).variance_of(db, typevar)
    }
}

/// A `ProtocolInstanceType` represents the set of all possible runtime objects
/// that conform to the interface described by a certain protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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
    match protocol.inner {
        Protocol::FromClass(class) if !visitor.should_visit_lazy_type_attributes() => {
            if let Some((_, Some(specialization))) = class.static_class_literal(db) {
                walk_specialization(db, specialization, visitor);
            }
        }
        Protocol::FromClass(_) | Protocol::Interface(_) => {
            walk_protocol_interface(db, protocol.inner.interface(db), visitor);
        }
    }

    if let Protocol::Interface(interface) = protocol.inner
        && let Some(origin) = interface.materialized_origin(db)
        && let Some((_, Some(specialization))) = origin.static_class_literal(db)
    {
        walk_specialization(db, specialization, visitor);
    }
}

/// Returns whether a protocol requirement refers back to its class-backed origin.
///
/// Aliases are followed, but nested protocol interfaces are not expanded. A recursive protocol
/// such as the following must retain its class-backed representation to avoid synthesizing a new
/// materialized wrapper on every mapping:
///
/// ```python
/// type Alias = P
///
/// class P(Protocol):
///     @property
///     def child(self) -> Alias: ...
/// ```
#[salsa::tracked(returns(copy))]
fn interface_references_protocol_origin<'db>(
    db: &'db dyn Db,
    interface: ProtocolInterface<'db>,
    protocol: ProtocolClass<'db>,
) -> bool {
    struct ProtocolReferenceFinder<'db> {
        protocol_origin: ClassLiteral<'db>,
        found: Cell<bool>,
        recursion_guard: TypeCollector<'db>,
    }

    impl<'db> TypeVisitor<'db> for ProtocolReferenceFinder<'db> {
        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }

        fn visit_type_alias_type(&self, db: &'db dyn Db, type_alias: TypeAliasType<'db>) {
            self.visit_type(db, type_alias.value_type(db));
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if self.found.get() {
                return;
            }

            if ty
                .as_protocol_instance()
                .and_then(|protocol| protocol.class_origin(db))
                .is_some_and(|origin| origin.class_literal(db) == self.protocol_origin)
            {
                self.found.set(true);
                return;
            }

            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }
    }

    let visitor = ProtocolReferenceFinder {
        protocol_origin: protocol.class_literal(db),
        found: Cell::new(false),
        recursion_guard: TypeCollector::default(),
    };
    walk_protocol_interface_requirements(db, interface, &visitor);
    visitor.found.get()
}

impl<'db> ProtocolInstanceType<'db> {
    /// Return whether this protocol has an interface produced by materialization.
    pub(super) fn is_materialized(self, db: &'db dyn Db) -> bool {
        matches!(self.inner, Protocol::Interface(interface) if interface.is_materialized(db))
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match self.inner {
            Protocol::Interface(interface) => interface.materialization_kind(db),
            Protocol::FromClass(_) => None,
        }
    }

    /// Returns the materialization wrapper that must be shown when displaying this protocol.
    ///
    /// A generic origin can already carry the wrapper in its specialization, while a covariant
    /// materialization can be represented directly by its rewritten origin. Only irreducible
    /// materialized interfaces need an additional `Top[...]` or `Bottom[...]` wrapper.
    pub(super) fn display_materialization_kind(
        self,
        db: &'db dyn Db,
    ) -> Option<MaterializationKind> {
        let kind = self.materialization_kind(db)?;
        let origin = self.materialized_origin(db)?;
        let origin_already_displays_kind = origin
            .static_class_literal(db)
            .and_then(|(_, specialization)| specialization)
            .and_then(|specialization| specialization.materialization_kind(db))
            .is_some();

        (!origin_already_displays_kind && self.interface(db) != origin.interface(db))
            .then_some(kind)
    }

    /// Returns whether materialization rewrote any member required by `target`.
    ///
    /// The nominal MRO shortcut remains valid when materialization changed only unrelated members;
    /// otherwise the effective interface must participate in the relation.
    fn materialization_changes_requirements(
        self,
        db: &'db dyn Db,
        target: ProtocolInterface<'db>,
    ) -> bool {
        let Protocol::Interface(interface) = self.inner else {
            return false;
        };
        let Some(origin) = interface.materialized_origin(db) else {
            return false;
        };
        interface
            .interface(db)
            .differs_for_members_required_by(db, origin.interface(db), target)
    }

    /// Return `true` if this is the standard-library `Hashable` protocol.
    pub(super) fn is_hashable(self, db: &'db dyn Db) -> bool {
        self.to_nominal_instance(db)
            .is_some_and(|instance| instance.class(db).is_known(db, KnownClass::Hashable))
    }

    /// Returns the class represented by this protocol, if it originated from a class definition.
    pub(super) fn class_origin(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        match self.inner {
            Protocol::FromClass(class) => Some(class),
            Protocol::Interface(interface) => interface.materialized_origin(db),
        }
    }

    /// Returns the origin of a materialized property for descriptor-aware assignment lookup.
    ///
    /// The effective interface carries mapped read and write types, but the origin retains
    /// descriptor identity and deletion behavior.
    pub(super) fn materialized_origin_property(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<ProtocolClass<'db>> {
        let origin = self.materialized_origin(db)?;
        origin
            .interface(db)
            .member_is_property(db, name)
            .then_some(origin)
    }

    pub(super) fn materialized_origin(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        match self.inner {
            Protocol::Interface(interface) => interface.materialized_origin(db),
            Protocol::FromClass(_) => None,
        }
    }

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
    fn synthesized(db: &'db dyn Db, interface: ProtocolInterface<'db>) -> Self {
        Self {
            inner: Protocol::Interface(ProtocolInterfaceType::synthesized(db, interface)),
            _phantom: PhantomData,
        }
    }

    fn materialized(
        db: &'db dyn Db,
        interface: ProtocolInterface<'db>,
        origin: ProtocolClass<'db>,
        materialization_kind: MaterializationKind,
    ) -> Self {
        Self {
            inner: Protocol::Interface(ProtocolInterfaceType::materialized(
                db,
                interface,
                origin,
                materialization_kind,
            )),
            _phantom: PhantomData,
        }
    }

    /// If this protocol corresponds to a class definition, convert it into a nominal instance.
    ///
    /// Pure synthesized protocols have no class origin and cannot be treated nominally.
    pub(super) fn to_nominal_instance(self, db: &'db dyn Db) -> Option<NominalInstanceType<'db>> {
        self.class_origin(db).map(|origin| {
            NominalInstanceType(NominalInstanceInner::NonTuple(NominalInstanceClass::Plain(
                *origin,
            )))
        })
    }

    /// Return the structural meta-type of this protocol-instance type.
    pub(super) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        match self.inner {
            Protocol::FromClass(_) => SubclassOfType::from_protocol(self),
            Protocol::Interface(interface) => interface.materialized_origin(db).map_or_else(
                || KnownClass::Type.to_instance(db),
                |_| SubclassOfType::from_protocol(self),
            ),
        }
    }

    /// Return the meta-type used for internal class-member lookup on a protocol instance.
    ///
    /// Class-backed protocols use their nominal origin for descriptor lookup. Materialized
    /// protocol members are handled through their effective interface before this fallback.
    pub(super) fn to_nominal_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        self.class_origin(db).map_or_else(
            || self.to_meta_type(db),
            |origin| SubclassOfType::from(db, *origin),
        )
    }

    /// Return `true` if this protocol is a supertype of `object`.
    ///
    /// This indicates that the protocol represents the same set of possible runtime objects
    /// as `object` (since `object` is the universal set of *all* possible runtime objects!).
    /// Such a protocol is therefore an equivalent type to `object`, which would in fact be
    /// normalised to `object`.
    pub(super) fn is_equivalent_to_object(self, db: &'db dyn Db) -> bool {
        #[salsa::tracked(returns(copy), cycle_initial=|_, _, _, ()| true, heap_size=ruff_memory_usage::heap_size)]
        fn is_equivalent_to_object_inner<'db>(
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
            _: (),
        ) -> bool {
            let constraints = ConstraintSetBuilder::new();
            let relation_visitor = HasRelationToVisitor::default(&constraints);
            let disjointness_visitor = IsDisjointVisitor::default(&constraints);
            let signature_relation_visitor = SignatureRelationVisitor::default();
            let materialization_visitor = ApplyTypeMappingVisitor::default();
            let checker = TypeRelationChecker::subtyping(
                &constraints,
                InferableTypeVars::None,
                &relation_visitor,
                &disjointness_visitor,
                &signature_relation_visitor,
                &materialization_visitor,
            );
            checker
                .check_type_satisfies_protocol(db, Type::object(), protocol)
                .is_always_satisfied(db)
        }

        is_equivalent_to_object_inner(db, self, ())
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

    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match self.inner {
            Protocol::FromClass(class) => class.instance_member(db, name),
            Protocol::Interface(interface) => {
                let protocol_interface = interface.interface(db);
                let Some(origin) = interface.materialized_origin(db) else {
                    return protocol_interface.instance_member(db, name);
                };
                if protocol_interface.includes_member(db, name)
                    && !origin.interface(db).member_has_todo_type(db, name)
                {
                    protocol_interface.instance_member(db, name)
                } else {
                    origin.instance_member(db, name)
                }
            }
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
                let materialization_kind = match type_mapping {
                    TypeMapping::Materialize(kind)
                    | TypeMapping::ApplySpecializationWithMaterialization {
                        materialization_kind: kind,
                        ..
                    } => *kind,
                    _ => {
                        return Self::from_class(class.apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            visitor,
                        ));
                    }
                };
                let interface = class.interface(db);
                let mapped_class = class.apply_type_mapping_impl(db, type_mapping, tcx, visitor);

                // A recursive reference is returned unchanged by the mapping visitor. Synthesizing
                // an interface that still contains the same protocol class, including under a
                // different specialization, would add a fresh wrapper every time it is mapped
                // again. Keep the stable class-backed representation until recursive protocol
                // materialization can preserve its origin without expanding indefinitely.
                // TODO: Remove this fallback once https://github.com/astral-sh/ruff/pull/24981 merges.
                if interface_references_protocol_origin(db, interface, class) {
                    return Self::from_class(mapped_class);
                }

                let mapped_interface =
                    interface.apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                if mapped_interface == interface
                    || interface_references_protocol_origin(db, mapped_interface, class)
                {
                    return Self::from_class(mapped_class);
                }

                Self::materialized(db, mapped_interface, mapped_class, materialization_kind)
            }
            Protocol::Interface(interface) => Self {
                inner: Protocol::Interface(interface.apply_type_mapping_impl(
                    db,
                    type_mapping,
                    tcx,
                    visitor,
                )),
                _phantom: PhantomData,
            },
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
            Protocol::Interface(interface) => {
                interface.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
    }

    pub(super) fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        self.inner.interface(db)
    }
}

impl<'db> VarianceInferable<'db> for ProtocolInstanceType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarIdentity<'db>) -> TypeVarVariance {
        self.inner.variance_of(db, typevar)
    }
}

/// The representation of a protocol class or an interned interface.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum Protocol<'db> {
    FromClass(ProtocolClass<'db>),
    Interface(ProtocolInterfaceType<'db>),
}

impl<'db> Protocol<'db> {
    /// Return the members of this protocol type
    fn interface(self, db: &'db dyn Db) -> ProtocolInterface<'db> {
        match self {
            Self::FromClass(class) => class.interface(db),
            Self::Interface(interface) => interface.interface(db),
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
            Self::Interface(interface) => Some(Self::Interface(
                interface.recursive_type_normalized_impl(db, div, nested)?,
            )),
        }
    }
}

impl<'db> VarianceInferable<'db> for Protocol<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarIdentity<'db>) -> TypeVarVariance {
        match self {
            Protocol::FromClass(class_type) => class_type.variance_of(db, typevar),
            Protocol::Interface(interface) => interface.variance_of(db, typevar),
        }
    }
}

mod interface_protocol {
    use crate::types::protocol_class::{ProtocolClass, ProtocolInterface};
    use crate::types::{
        ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance,
        FindLegacyTypeVarsVisitor, MaterializationKind, Type, TypeContext, TypeMapping,
        TypeVarVariance, VarianceInferable,
    };
    use crate::{Db, FxOrderSet};
    use ty_python_core::definition::Definition;

    /// Identifies whether an interned interface is a pure synthesized protocol or the materialized
    /// view of a class-based protocol.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
    pub(in crate::types) enum ProtocolInterfaceKind<'db> {
        Materialized {
            origin: ProtocolClass<'db>,
            materialization_kind: MaterializationKind,
        },
        Synthesized,
    }

    /// An interned protocol interface.
    ///
    /// Keeping the materialized/synthesized distinction inside this interned value lets the
    /// enclosing [`super::Protocol`] retain its compact two-variant representation.
    #[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
    pub(in crate::types) struct ProtocolInterfaceType<'db> {
        #[returns(copy)]
        pub(in crate::types) interface: ProtocolInterface<'db>,
        #[returns(copy)]
        pub(in crate::types) kind: ProtocolInterfaceKind<'db>,
    }

    impl get_size2::GetSize for ProtocolInterfaceType<'_> {}

    impl<'db> ProtocolInterfaceType<'db> {
        pub(super) fn synthesized(db: &'db dyn Db, interface: ProtocolInterface<'db>) -> Self {
            Self::new(db, interface, ProtocolInterfaceKind::Synthesized)
        }

        pub(super) fn materialized(
            db: &'db dyn Db,
            interface: ProtocolInterface<'db>,
            origin: ProtocolClass<'db>,
            materialization_kind: MaterializationKind,
        ) -> Self {
            Self::new(
                db,
                interface,
                ProtocolInterfaceKind::Materialized {
                    origin,
                    materialization_kind,
                },
            )
        }

        pub(super) fn is_materialized(self, db: &'db dyn Db) -> bool {
            matches!(self.kind(db), ProtocolInterfaceKind::Materialized { .. })
        }

        pub(super) fn materialized_origin(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
            match self.kind(db) {
                ProtocolInterfaceKind::Materialized { origin, .. } => Some(origin),
                ProtocolInterfaceKind::Synthesized => None,
            }
        }

        pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
            match self.kind(db) {
                ProtocolInterfaceKind::Materialized {
                    materialization_kind,
                    ..
                } => Some(materialization_kind),
                ProtocolInterfaceKind::Synthesized => None,
            }
        }

        /// Remaps the effective interface and origin metadata while preserving the first
        /// materialization's polarity, making nested top/bottom materialization idempotent.
        pub(super) fn apply_type_mapping_impl<'a>(
            self,
            db: &'db dyn Db,
            type_mapping: &TypeMapping<'a, 'db>,
            tcx: TypeContext<'db>,
            visitor: &ApplyTypeMappingVisitor<'db>,
        ) -> Self {
            let kind = match self.kind(db) {
                ProtocolInterfaceKind::Materialized {
                    origin,
                    materialization_kind,
                } => ProtocolInterfaceKind::Materialized {
                    origin: origin.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                    materialization_kind,
                },
                ProtocolInterfaceKind::Synthesized => ProtocolInterfaceKind::Synthesized,
            };
            Self::new(
                db,
                self.interface(db)
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                kind,
            )
        }

        /// Collects variables from both the effective interface and the preserved origin
        /// specialization, where variables omitted from protocol requirements can remain.
        pub(super) fn find_legacy_typevars_impl(
            self,
            db: &'db dyn Db,
            binding_context: Option<Definition<'db>>,
            typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
            visitor: &FindLegacyTypeVarsVisitor<'db>,
        ) {
            self.interface(db)
                .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            if let Some(origin) = self.materialized_origin(db) {
                origin.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }

        pub(in crate::types) fn recursive_type_normalized_impl(
            self,
            db: &'db dyn Db,
            div: Type<'db>,
            nested: bool,
        ) -> Option<Self> {
            let kind = match self.kind(db) {
                ProtocolInterfaceKind::Materialized {
                    origin,
                    materialization_kind,
                } => ProtocolInterfaceKind::Materialized {
                    origin: origin.recursive_type_normalized_impl(db, div, nested)?,
                    materialization_kind,
                },
                ProtocolInterfaceKind::Synthesized => ProtocolInterfaceKind::Synthesized,
            };
            Some(Self::new(
                db,
                self.interface(db)
                    .recursive_type_normalized_impl(db, div, nested)?,
                kind,
            ))
        }
    }

    impl<'db> VarianceInferable<'db> for ProtocolInterfaceType<'db> {
        fn variance_of(
            self,
            db: &'db dyn Db,
            typevar: BoundTypeVarIdentity<'db>,
        ) -> TypeVarVariance {
            self.interface(db).variance_of(db, typevar)
        }
    }
}
