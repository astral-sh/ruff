//! Instance types: both nominal and structural.

use crate::types::mapping::effects::{
    InlineMappingEffects, MappingEffects, SynchronousMappingEffects, inline_mapping_result,
};

use crate::ProgramEnvironment;
use std::borrow::Cow;
use std::cell::Cell;
use std::marker::PhantomData;

use self::effects::{InstanceEffects, InstanceWork, LegacyInlineEffects};
use self::normalization::{NominalNormalizationFacts, nominal_normalize_sync};
use self::protocol_object::{InlineProtocolObjectEffects, protocol_object_equivalence_sync};
use super::protocol_class::{ProtocolInterface, ProtocolInterfaceView};
use super::{
    BoundTypeVarIdentity, ClassType, DivergentType, GenericAlias, KnownClass,
    MaterializationKind, StaticClassLiteral, SubclassOfType, Type, TypeAliasType,
};
use crate::place::PlaceAndQualifiers;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::cyclic::{ActiveRecursionDetector, TypeIdentity};
use crate::types::generics::{Specialization, walk_specialization};
use crate::types::normalization::OrdinaryNormalizationEffects;
use crate::types::promotion::classification::SingletonRepresentation;
use crate::types::promotion::{
    InlinePublicPromotionEffects, PublicPromotionFacts, inline_public_promotion_result,
};
use crate::types::protocol_class::{
    ProtocolClass, walk_protocol_instance_member, walk_protocol_interface,
};
use crate::types::relation::{
    DisjointnessChecker, HasRelationToVisitor, IsDisjointVisitor, RelationFieldReads,
    TypeRelationChecker,
};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::signatures::effects::legacy_inline;
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_spec};
use crate::types::visitor::{
    TypeCollector, TypeVisitor, materialization_is_noop, walk_type_with_recursion_guard,
};
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, ClassLiteral,
    LiteralValueTypeKind, TypeContext, TypeMapping, VarianceInferable, VarianceTerm,
};
use crate::Db;
pub(super) use synthesized_protocol::SynthesizedProtocolType;

pub(in crate::types) mod effects;
pub(in crate::types) mod mapping;
pub(in crate::types) mod nominal_relation;
pub(in crate::types) mod normalization;
pub(in crate::types) mod protocol_object;
pub(in crate::types) mod protocol_relation;
pub(in crate::types) mod tuple_spec;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod runtime;

#[cfg(test)]
pub(in crate::types) mod attempt;

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

    pub(crate) fn instance(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Self {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::mro_effects_enabled() {
            return attempt::instance(db, env, class).unwrap_or_else(|_| Type::unknown());
        }
        legacy_inline(Self::instance_with(db, env, &LegacyInlineEffects, class))
    }

    pub(in crate::types) async fn instance_with<E: InstanceEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        class: ClassType<'db>,
    ) -> Result<Self, E::Error> {
        effects.checkpoint(InstanceWork::Dispatch).await?;
        let (literal, specialization) = effects.class_literal_and_specialization(db, class).await?;
        Ok(match literal {
            // Dynamic classes created via `type()` don't have special instance types.
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicEnum(_) => {
                let inherits_explicit_any = effects.inherits_from_explicit_any(db, literal).await?;
                effects.checkpoint(InstanceWork::Publish).await?;
                Type::NominalInstance(NominalInstanceType::from_class_with_inheritance(
                    db,
                    class,
                    inherits_explicit_any,
                ))
            }
            // Functional TypedDicts return a TypedDict instance type.
            ClassLiteral::DynamicTypedDict(_) => {
                effects.checkpoint(InstanceWork::Publish).await?;
                Type::typed_dict(class)
            }
            ClassLiteral::Static(class_literal) => {
                match effects.known_class(db, class_literal).await? {
                    Some(KnownClass::Tuple) => {
                        let tuple = effects.tuple(db, env, specialization).await?;
                        effects.checkpoint(InstanceWork::Publish).await?;
                        Type::tuple(tuple)
                    }
                    Some(KnownClass::Object) => {
                        effects.checkpoint(InstanceWork::Publish).await?;
                        Type::object()
                    }
                    _ => {
                        if effects.is_typed_dict(db, class_literal).await? {
                            effects.checkpoint(InstanceWork::Publish).await?;
                            Type::typed_dict(class)
                        } else if effects.is_protocol(db, class_literal).await? {
                            effects.checkpoint(InstanceWork::Publish).await?;
                            Self::ProtocolInstance(ProtocolInstanceType::from_class(
                                ProtocolClass::from_class(class),
                            ))
                        } else {
                            let inherits_explicit_any =
                                effects.inherits_from_explicit_any(db, literal).await?;
                            effects.checkpoint(InstanceWork::Publish).await?;
                            Type::NominalInstance(NominalInstanceType::from_class_with_inheritance(
                                db,
                                class,
                                inherits_explicit_any,
                            ))
                        }
                    }
                }
            }
        })
    }

    pub(crate) fn tuple(tuple: TupleType<'db>) -> Self {
        Type::NominalInstance(NominalInstanceType(NominalInstanceInner::ExactTuple(tuple)))
    }

    pub fn homogeneous_tuple(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        element: Type<'db>,
    ) -> Self {
        Type::tuple(TupleType::homogeneous(db, env, element))
    }

    pub(crate) fn heterogeneous_tuple<I, T>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        elements: I,
    ) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        Type::tuple(TupleType::heterogeneous(
            db,
            env,
            elements.into_iter().map(Into::into),
        ))
    }

    pub(crate) fn empty_tuple(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Type::tuple(TupleType::empty(db, env))
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
            Type::NominalInstance(instance) => instance.has_known_class(db, known_class),
            _ => false,
        }
    }

    /// Synthesize a protocol instance type with a given set of read-only property members.
    pub(super) fn protocol_with_readonly_members<'a, M>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        members: M,
    ) -> Self
    where
        M: IntoIterator<Item = (&'a str, Type<'db>)>,
    {
        Self::ProtocolInstance(ProtocolInstanceType::synthesized(
            SynthesizedProtocolType::new(ProtocolInterface::with_property_members(
                db, env, members,
            )),
        ))
    }

    /// Synthesize a protocol instance type with a given set of methods.
    pub(super) fn protocol_with_methods<'a, M>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        methods: M,
    ) -> Self
    where
        M: IntoIterator<Item = (&'a str, CallableType<'db>)>,
    {
        Self::ProtocolInstance(ProtocolInstanceType::synthesized(
            SynthesizedProtocolType::new(ProtocolInterface::with_methods(db, env, methods)),
        ))
    }

    /// Return the constructed type used in meta-protocol matching and inference.
    ///
    /// There are no constructor arguments here from which to infer the class's type arguments.
    /// Use its defaults, as ordinary class-member lookup does, so that class-scoped typevars
    /// do not escape through the constructor return type into protocol inference.
    pub(super) fn instance_type_for_meta_protocol(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        let constructor_ty = self.to_class_type(db).map_or(self, Type::from);
        constructor_ty.bindings(db, env).return_type(db, env)
    }
}

/// A type representing the set of runtime objects which are instances of a certain nominal class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct NominalInstanceType<'db>(
    // Keep this field private, so that the only way of constructing `NominalInstanceType` instances
    // is through the `Type::instance` constructor function.
    NominalInstanceInner<'db>,
);

#[derive(Clone, Copy)]
pub(in crate::types) enum NominalVisitorKind<'db> {
    None,
    Class(NominalInstanceClass<'db>),
    Tuple(TupleType<'db>),
}

#[derive(Clone, Copy)]
pub(super) enum NominalVisitorChildren<'db> {
    None,
    Class(Type<'db>),
    Tuple(&'db TupleSpec<'db>),
}

pub(super) fn walk_nominal_instance_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    nominal: NominalInstanceType<'db>,
    visitor: &V,
) {
    match nominal.children_for_visitor(db) {
        NominalVisitorChildren::Tuple(tuple) => {
            walk_tuple_spec(db, tuple, visitor);
        }
        NominalVisitorChildren::Class(class) => {
            visitor.visit_type(db, class);
        }
        NominalVisitorChildren::None => {}
    }
}

impl<'db> NominalInstanceType<'db> {
    pub(in crate::types) fn visitor_kind(self) -> NominalVisitorKind<'db> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => NominalVisitorKind::Tuple(tuple),
            NominalInstanceInner::NonTuple(class) => NominalVisitorKind::Class(class),
            NominalInstanceInner::Object | NominalInstanceInner::SysVersionInfo => {
                NominalVisitorKind::None
            }
        }
    }

    pub(super) fn children_for_visitor(self, db: &'db dyn Db) -> NominalVisitorChildren<'db> {
        self.children_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn children_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> NominalVisitorChildren<'db> {
        match self.visitor_kind() {
            NominalVisitorKind::Tuple(tuple) => {
                NominalVisitorChildren::Tuple(tuple.read_fields(fields).tuple())
            }
            NominalVisitorKind::None => NominalVisitorChildren::None,
            NominalVisitorKind::Class(class) => {
                let class = match class {
                    NominalInstanceClass::Plain(class) => class,
                    NominalInstanceClass::InheritsFromExplicitAny(class) => {
                        *class.read_fields(fields).class()
                    }
                };
                NominalVisitorChildren::Class(class.into())
            }
        }
    }

    fn from_class_with_inheritance(
        db: &'db dyn Db,
        class: ClassType<'db>,
        inherits_explicit_any: bool,
    ) -> Self {
        Self(NominalInstanceInner::NonTuple(
            NominalInstanceClass::from_class_with_inheritance(db, class, inherits_explicit_any),
        ))
    }

    /// Return whether this instance's class inherits from an explicit `Any` base.
    pub(super) const fn inherits_from_explicit_any(self) -> bool {
        match self.0 {
            NominalInstanceInner::NonTuple(class) => class.inherits_from_explicit_any(),
            _ => false,
        }
    }

    pub(super) fn class(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> ClassType<'db> {
        match nominal_class_sync(*self, NominalClassFacts, &InlineNominalClass { db, env }) {
            Ok(class) => class,
            Err(error) => match error {},
        }
    }

    /// Returns the class literal for this instance.
    pub(super) fn class_literal(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> ClassLiteral<'db> {
        self.class(db, env).class_literal(db)
    }

    /// Returns the [`KnownClass`] that this is a nominal instance of, or `None` if it is not an
    /// instance of a known class.
    pub(super) fn known_class(&self, db: &'db dyn Db) -> Option<KnownClass> {
        self.known_class_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn known_class_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> Option<KnownClass> {
        match nominal_known_class_sync(self, NominalClassFacts, &InlineNominalKnownClass { fields })
        {
            Ok(class) => class,
            Err(error) => match error {},
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
    pub(super) fn tuple_spec(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Cow<'db, TupleSpec<'db>>> {
        match tuple_spec::nominal_tuple_spec_sync(
            *self,
            env,
            tuple_spec::TupleSpecFacts,
            &tuple_spec::OrdinaryTupleSpecEffects { db },
        ) {
            Ok(tuple) => tuple,
            Err(never) => match never {},
        }
    }

    /// Return `true` if this type represents instances of the class `builtins.object`.
    pub(super) const fn is_object(self) -> bool {
        matches!(self.0, NominalInstanceInner::Object)
    }

    pub(super) fn is_definition_generic(self, db: &'db dyn Db) -> bool {
        match nominal_is_definition_generic_sync(
            self,
            NominalClassFacts,
            &InlineNominalGeneric { db },
        ) {
            Ok(is_generic) => is_generic,
            Err(error) => match error {},
        }
    }

    pub(in crate::types) const fn exact_tuple(self) -> Option<TupleType<'db>> {
        match self.0 {
            NominalInstanceInner::ExactTuple(tuple) => Some(tuple),
            _ => None,
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
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match nominal_normalize_sync(
            self,
            env,
            div,
            nested,
            &OrdinaryNormalizationEffects { db },
            NominalNormalizationFacts,
        ) {
            Ok(normalized) => normalized,
            Err(error) => match error {},
        }
    }

    pub(super) fn is_singleton(self, db: &'db dyn Db) -> bool {
        inline_public_promotion_result(self.is_singleton_with(db, &InlinePublicPromotionEffects))
    }

    pub(super) fn is_singleton_with<E: PublicPromotionFacts<'db>>(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<bool, E::Error> {
        crate::types::promotion::classification::classify_singleton_sync(
            self,
            crate::types::promotion::classification::SingletonFacts,
            &crate::types::promotion::classification::OrdinarySingletonEffects { db, facts: effects },
        )
    }

    /// Returns the inline nominal representation used by shared singleton classification.
    pub(in crate::types) const fn singleton_representation(self) -> SingletonRepresentation<'db> {
        match self.0 {
            // The empty tuple is a singleton on CPython and PyPy, but not on other Python
            // implementations such as GraalPy. Its *use* as a singleton is discouraged and
            // should not be relied on for type narrowing, so we do not treat it as one.
            // See:
            // https://docs.python.org/3/reference/expressions.html#parenthesized-forms
            NominalInstanceInner::ExactTuple(_) => SingletonRepresentation::ExactTuple,
            NominalInstanceInner::Object => SingletonRepresentation::Object,
            NominalInstanceInner::SysVersionInfo => SingletonRepresentation::SysVersionInfo,
            NominalInstanceInner::NonTuple(class) => SingletonRepresentation::NonTuple(class),
        }
    }

    pub(super) fn to_meta_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        SubclassOfType::from(db, env, self.class(db, env))
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        inline_mapping_result(self.apply_type_mapping_sync(
            db,
            type_mapping,
            tcx,
            visitor,
            &InlineMappingEffects,
        ))
    }

    pub(super) async fn apply_type_mapping_with<'a, E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        mapping::map_nominal_with(
            db,
            self,
            type_mapping,
            tcx,
            visitor,
            &mapping::MappingNominalEffects(effects),
            mapping::NominalMappingFacts,
        )
        .await
    }

    /// Applies the shared nominal mapping decisions with synchronous dependency effects.
    pub(super) fn apply_type_mapping_sync<'a, E: SynchronousMappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        mapping::map_nominal_sync(
            db,
            self,
            type_mapping,
            tcx,
            visitor,
            &mapping::MappingNominalEffects(effects),
            mapping::NominalMappingFacts,
        )
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
        match protocol_relation::check_type_satisfies_protocol_sync(
            RelationFieldReads::new(db),
            self,
            ty,
            protocol,
            &protocol_relation::InlineProtocolRelationEffects::new(
                db,
                &crate::types::relation::dependencies::OrdinaryDependencies,
            ),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Try a nominal proof when a materialized recursive protocol changes specialization.
    ///
    /// A recursive child can stabilize at a specialization that relates nominally even when its
    /// parent only relates structurally. Keep the child's constraints without retrying the
    /// structural comparison that reached the recursion guard.
    pub(super) fn try_check_nominal_protocol_cycle(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        let source = source.as_protocol_instance()?;
        let target = target.as_protocol_instance()?;
        if source.materialization_kind(db).is_none() && target.materialization_kind(db).is_none() {
            return None;
        }
        let source_origin = source.class_origin(db)?;
        let target_origin = target.class_origin(db)?;
        if source_origin.class_literal(db) != target_origin.class_literal(db) {
            return None;
        }

        // Nominal arguments alone do not describe materialized requirements such as a fixed
        // `Any` member. Only use the nominal proof when the pending wrappers are harmless.
        for protocol in [source, target] {
            if let Some(origin) = protocol.materialized_origin(db)
                && !materialization_is_noop(
                    db,
                    self.env,
                    Type::ProtocolInstance(ProtocolInstanceType::from_class(origin)),
                )
            {
                return None;
            }
        }

        Some(self.check_type_pair(
            db,
            Type::NominalInstance(source.nominal_origin_instance(db)?),
            Type::NominalInstance(target.nominal_origin_instance(db)?),
        ))
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
        match protocol_relation::check_meta_type_satisfies_protocol_sync(
            RelationFieldReads::new(db),
            self,
            meta_ty,
            protocol,
            &protocol_relation::InlineProtocolRelationEffects::new(
                db,
                &crate::types::relation::dependencies::OrdinaryDependencies,
            ),
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    pub(super) fn check_nominal_instance_pair(
        &self,
        db: &'db dyn Db,
        source: NominalInstanceType<'db>,
        target: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match nominal_relation::check_nominal_pair_sync(
            source,
            target,
            nominal_relation::NominalPairFacts,
            &nominal_relation::InlineNominalPairs { db, checker: self },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
}

/// Returns the finite members of a protocol interface, omitting members that refer back to its
/// class-backed origin. Type aliases are expanded, but lazy protocol attributes are not visited.
///
/// For example, `value` is retained while `child` is omitted:
///
/// ```python
/// class P[T](Protocol):
///     def value(self) -> T | int: ...
///     def child(self) -> P[list[T]]: ...
/// ```
#[salsa::tracked(attempt = ReturnOnly, returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn non_recursive_protocol_interface<'db>(
    db: &'db dyn Db,
    interface: ProtocolInterface<'db>,
    protocol: ProtocolClass<'db>,
    receiver_ty: Type<'db>,
) -> ProtocolInterface<'db> {
    struct ProtocolReferenceFinder<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        origin: ClassLiteral<'db>,
        found: Cell<bool>,
        recursion_guard: TypeCollector<'db>,
        active_aliases: ActiveRecursionDetector<TypeIdentity<'db>>,
    }

    impl<'db> TypeVisitor<'db> for ProtocolReferenceFinder<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }

        fn visit_type_alias_type(&self, db: &'db dyn Db, type_alias: TypeAliasType<'db>) {
            self.active_aliases.visit(
                &Type::TypeAlias(type_alias).to_type_identity(db),
                || self.found.set(true),
                || self.visit_type(db, type_alias.value_type(db)),
            );
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            if self.found.get() {
                return;
            }

            if ty
                .as_protocol_instance()
                .and_then(|protocol| protocol.nominal_origin_instance(db))
                .is_some_and(|instance| {
                    instance.class_literal(db, self.program_environment()) == self.origin
                })
            {
                self.found.set(true);
                return;
            }

            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }
    }

    let env = ProgramEnvironment::from_file(protocol.class_literal(db).program_file(db));
    interface.filter_members(db, |member| {
        let visitor = ProtocolReferenceFinder {
            env: &env,
            origin: protocol.class_literal(db),
            found: Cell::new(false),
            recursion_guard: TypeCollector::default(),
            active_aliases: ActiveRecursionDetector::default(),
        };
        walk_protocol_instance_member(db, member, receiver_ty, &visitor);
        !visitor.found.get()
    })
}

/// Infers protocol constraints without expanding recursive member requirements.
///
/// The target view retains its materialization, so readable and writable members are still
/// materialized in their respective variance positions. The complete target protocol must be
/// checked separately after generic inference.
#[salsa::tracked(
    attempt = ReturnOnly,
    returns(ref),
    cycle_initial = |_, _, _, _| OwnedConstraintSet::always(),
    heap_size = ruff_memory_usage::heap_size,
)]
fn non_recursive_protocol_constraints<'db>(
    db: &'db dyn Db,
    source: ProtocolInstanceType<'db>,
    target: ProtocolInterfaceView<'db>,
) -> OwnedConstraintSet<'db> {
    let env = ProgramEnvironment::from_program(target.base().program(db));
    let constraints = ConstraintSetBuilder::new();
    constraints.into_owned(|constraints| {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::new(&env);
        let checker = TypeRelationChecker::constraint_set_assignability(
            &env,
            constraints,
            &relation_visitor,
            &disjointness_visitor,
            &signature_relation_visitor,
            &materialization_visitor,
        );
        checker.check_protocol_interface_pair(
            db,
            Type::ProtocolInstance(source),
            source.interface(db),
            target,
        )
    })
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
        let env = self.env;
        if let Some(left_spec) = left.tuple_spec(db, env)
            && let Some(right_spec) = right.tuple_spec(db, env)
        {
            let compatible = self.check_tuple_spec_pair(db, &left_spec, &right_spec);
            if result
                .union(db, self.constraints, compatible)
                .is_trivially_always_satisfied()
            {
                return result;
            }
        }

        result.or(db, self.constraints, || {
            ConstraintSet::from_bool(
                self.constraints,
                !left
                    .class(db, env)
                    .could_coexist_in_mro_with_disjointness_checker(
                        db,
                        env,
                        right.class(db, env),
                        self,
                    ),
            )
        })
    }
}

/// The class of a nominal instance whose MRO contains an explicit `Any` base.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size, field_view=read_fields, field_requests=field_requests)]
pub(in crate::types) struct ExplicitAnyInstanceClass<'db> {
    #[returns(copy)]
    pub(in crate::types) class: ClassType<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ExplicitAnyInstanceClass<'_> {}

/// The class stored by a non-tuple nominal instance.
///
/// Interning the uncommon explicit-`Any` case lets this type store the additional semantic bit
/// without increasing the size of [`Type`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(in crate::types) enum NominalInstanceClass<'db> {
    Plain(ClassType<'db>),
    InheritsFromExplicitAny(ExplicitAnyInstanceClass<'db>),
}

impl<'db> NominalInstanceClass<'db> {
    fn from_class_with_inheritance(
        db: &'db dyn Db,
        class: ClassType<'db>,
        inherits_explicit_any: bool,
    ) -> Self {
        if inherits_explicit_any {
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

fn sys_version_info_class<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> Option<ClassType<'db>> {
    KnownClass::VersionInfo
        .try_to_class_literal(db, env)
        .map(|class| class.default_specialization(db))
}

pub(crate) struct SliceLiteral {
    pub(crate) start: Option<i32>,
    pub(crate) stop: Option<i32>,
    pub(crate) step: Option<i32>,
}

impl<'db> VarianceInferable<'db> for NominalInstanceType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        self.class(db, env).variance_of(db, env, typevar)
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

/// A test representation for constructing or inspecting stored protocols without resolving members.
#[cfg(test)]
pub(in crate::types) enum ProtocolInterfaceSource<'db> {
    Class(ProtocolClass<'db>),
    Synthesized(ProtocolInterface<'db>),
    Materialized {
        origin: ProtocolClass<'db>,
        kind: MaterializationKind,
    },
}

// All variants contain only enum tags and interned handles: class origins (including generic
// aliases), synthesized interfaces, or materialized protocol handles. Derived Hash and Eq never
// follow their class fields, type arguments, or member maps.
#[cfg(test)]
impl salsa::plumbing::function::FixedQueryFields for ProtocolInstanceType<'_> {}

#[derive(Clone, Copy)]
pub(super) enum ProtocolVisitorChildren<'db> {
    Interface(ProtocolInterfaceView<'db>),
    Specialization(Option<Specialization<'db>>),
}

pub(super) fn walk_protocol_instance_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    protocol: ProtocolInstanceType<'db>,
    visitor: &V,
) {
    let include_lazy = visitor.should_visit_lazy_type_attributes();
    if !include_lazy
        && matches!(
            protocol.inner,
            Protocol::FromClass(_) | Protocol::Materialized(_)
        )
    {
        visitor.notify_skipped_lazy_type_attributes();
    }
    match protocol.children_for_visitor(db, include_lazy) {
        ProtocolVisitorChildren::Interface(interface) => {
            walk_protocol_interface(db, interface, visitor);
        }
        ProtocolVisitorChildren::Specialization(Some(specialization)) => {
            walk_specialization(db, specialization, visitor);
        }
        ProtocolVisitorChildren::Specialization(None) => {}
    }
}

impl<'db> ProtocolInstanceType<'db> {
    /// Constructs a stored protocol representation for tests without resolving its members.
    #[cfg(test)]
    pub(in crate::types) fn from_interface_source_for_test(
        db: &'db dyn Db,
        source: ProtocolInterfaceSource<'db>,
    ) -> Self {
        match source {
            ProtocolInterfaceSource::Class(class) => Self::from_class(class),
            ProtocolInterfaceSource::Synthesized(interface) => {
                Self::synthesized(SynthesizedProtocolType::new(interface))
            }
            ProtocolInterfaceSource::Materialized { origin, kind } => {
                Self::materialized(db, origin, kind)
            }
        }
    }

    pub(super) fn children_for_visitor(
        self,
        db: &'db dyn Db,
        include_lazy: bool,
    ) -> ProtocolVisitorChildren<'db> {
        if include_lazy {
            return ProtocolVisitorChildren::Interface(self.interface(db));
        }
        self.children_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn children_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> ProtocolVisitorChildren<'db> {
        match self.inner {
            Protocol::FromClass(_) | Protocol::Materialized(_) => {
                ProtocolVisitorChildren::Specialization(
                    self.class_origin_with_fields(fields)
                        .and_then(|class| match *class {
                            ClassType::NonGeneric(_) => None,
                            ClassType::Generic(alias) => {
                                Some(*alias.read_fields(fields).specialization())
                            }
                        }),
                )
            }
            Protocol::Synthesized(synthesized) => ProtocolVisitorChildren::Interface(
                ProtocolInterfaceView::new(synthesized.interface(), None),
            ),
        }
    }

    /// Return `true` if this is the standard-library `Hashable` protocol.
    pub(super) fn is_hashable(self, db: &'db dyn Db) -> bool {
        self.class_origin(db)
            .is_some_and(|class| class.is_known(db, KnownClass::Hashable))
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
    fn synthesized(synthesized: SynthesizedProtocolType<'db>) -> Self {
        Self {
            inner: Protocol::Synthesized(synthesized),
            _phantom: PhantomData,
        }
    }

    /// Preserves a class-based protocol and the polarity of its pending materialization.
    ///
    /// Member requirements are materialized only when an operation observes them.
    fn materialized(
        db: &'db dyn Db,
        origin: ProtocolClass<'db>,
        materialization_kind: MaterializationKind,
    ) -> Self {
        Self {
            inner: Protocol::Materialized(MaterializedProtocolType::new(
                db,
                origin,
                materialization_kind,
            )),
            _phantom: PhantomData,
        }
    }

    /// Returns the nominal instance of a protocol's origin without asserting nominal subtyping.
    pub(super) fn nominal_origin_instance(
        self,
        db: &'db dyn Db,
    ) -> Option<NominalInstanceType<'db>> {
        self.class_origin(db).map(|origin| {
            NominalInstanceType(NominalInstanceInner::NonTuple(NominalInstanceClass::Plain(
                *origin,
            )))
        })
    }

    pub(in crate::types) fn nominal_origin_instance_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> Option<NominalInstanceType<'db>> {
        self.class_origin_with_fields(fields).map(|origin| {
            NominalInstanceType(NominalInstanceInner::NonTuple(NominalInstanceClass::Plain(
                *origin,
            )))
        })
    }

    pub(in crate::types) fn class_origin_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> Option<ProtocolClass<'db>> {
        match self.inner {
            Protocol::FromClass(class) => Some(class),
            Protocol::Synthesized(_) => None,
            Protocol::Materialized(materialized) => {
                Some(*materialized.read_fields(fields).origin())
            }
        }
    }

    #[cfg(test)]
    pub(in crate::types) fn interface_source_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> ProtocolInterfaceSource<'db> {
        match self.inner {
            Protocol::FromClass(class) => ProtocolInterfaceSource::Class(class),
            Protocol::Synthesized(synthesized) => {
                ProtocolInterfaceSource::Synthesized(synthesized.interface())
            }
            Protocol::Materialized(materialized) => {
                let fields = materialized.read_fields(fields);
                ProtocolInterfaceSource::Materialized {
                    origin: *fields.origin(),
                    kind: *fields.materialization_kind(),
                }
            }
        }
    }

    pub(in crate::types) fn materialization_kind_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> Option<MaterializationKind> {
        match self.inner {
            Protocol::Materialized(materialized) => {
                Some(*materialized.read_fields(fields).materialization_kind())
            }
            Protocol::FromClass(_) | Protocol::Synthesized(_) => None,
        }
    }

    /// Return the class that defines this protocol, if it is class-backed.
    pub(super) fn class_origin(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        match self.inner {
            Protocol::FromClass(class) => Some(class),
            Protocol::Synthesized(_) => None,
            Protocol::Materialized(materialized) => Some(materialized.origin(db)),
        }
    }

    /// Returns the pending materialization of a class-based protocol, if any.
    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match self.inner {
            Protocol::Materialized(materialized) => Some(materialized.materialization_kind(db)),
            Protocol::FromClass(_) | Protocol::Synthesized(_) => None,
        }
    }

    /// Returns the class origin of a protocol with a pending materialization.
    pub(super) fn materialized_origin(self, db: &'db dyn Db) -> Option<ProtocolClass<'db>> {
        match self.inner {
            Protocol::Materialized(materialized) => Some(materialized.origin(db)),
            Protocol::FromClass(_) | Protocol::Synthesized(_) => None,
        }
    }

    /// Returns the nominal origin when a materialized requirement is a property descriptor.
    ///
    /// Descriptor lookup needs the original property object even though ordinary reads expose
    /// its lazily materialized value.
    pub(super) fn materialized_origin_property(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<ProtocolClass<'db>> {
        self.materialized_origin(db)
            .filter(|_| self.interface(db).member_is_property(db, name))
    }

    /// Returns whether a materialization changes any member required by `target`.
    ///
    /// An unrelated changed member must not prevent an explicitly inherited protocol from
    /// satisfying its base nominally.
    fn materialization_changes_requirements(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> bool {
        self.materialization_kind(db).is_some()
            && self
                .interface(db)
                .differs_for_members_required_by(db, env, target.interface(db))
    }

    /// Returns the materialization wrapper needed for displaying this protocol.
    ///
    /// Fully static requirements need no wrapper. A generic specialization can already display
    /// its materialization, in which case adding another wrapper would duplicate `Top` or
    /// `Bottom`.
    pub(super) fn display_materialization_kind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<MaterializationKind> {
        let Protocol::Materialized(materialized) = self.inner else {
            return None;
        };
        let origin = materialized.origin(db);
        if origin
            .static_class_literal(db)
            .and_then(|(_, specialization)| specialization)
            .and_then(|specialization| specialization.materialization_kind(db))
            .is_some()
        {
            return None;
        }

        let interface = self.interface(db);
        interface
            .differs_for_members_required_by(db, env, interface)
            .then_some(materialized.materialization_kind(db))
    }

    /// Return the structural meta-type of this protocol-instance type.
    pub(super) fn to_meta_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self.inner {
            Protocol::FromClass(_) | Protocol::Materialized(_) => {
                SubclassOfType::from_protocol(self)
            }

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
            Protocol::Synthesized(_) => KnownClass::Type.to_instance(db, env),
        }
    }

    /// Return the nominal meta-type used for internal class-member lookup on a protocol instance.
    pub(super) fn to_nominal_meta_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.class_origin(db).map_or_else(
            || self.to_meta_type(db, env),
            |origin| SubclassOfType::from(db, env, *origin),
        )
    }

    /// Return `true` if this protocol is a supertype of `object`.
    ///
    /// This indicates that the protocol represents the same set of possible runtime objects
    /// as `object` (since `object` is the universal set of *all* possible runtime objects!).
    /// Such a protocol is therefore an equivalent type to `object`, which would in fact be
    /// normalised to `object`.
    pub(super) fn is_equivalent_to_object(self, db: &'db dyn Db) -> bool {
        is_equivalent_to_object_inner(db, self, ())
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            inner: self
                .inner
                .recursive_type_normalized_impl(db, env, div, nested)?,
            _phantom: PhantomData,
        })
    }

    /// Returns an effective materialized member without applying the nominal class fallback.
    fn materialized_interface_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Option<PlaceAndQualifiers<'db>> {
        self.materialization_kind(db)?;
        let interface = self.interface(db);
        interface
            .includes_member(db, name)
            .then(|| interface.instance_member(db, env, name))
    }

    pub(crate) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        match self.inner {
            Protocol::FromClass(class) => class.instance_member(db, env, name),
            Protocol::Synthesized(synthesized) => {
                synthesized.interface().instance_member(db, env, name)
            }
            Protocol::Materialized(materialized) => self
                .materialized_interface_member(db, env, name)
                .unwrap_or_else(|| materialized.origin(db).instance_member(db, env, name)),
        }
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        match self.inner {
            Protocol::FromClass(class) => {
                let mapped_class = class.apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                if let TypeMapping::Materialize(materialization_kind) = type_mapping {
                    Self::materialized(db, mapped_class, *materialization_kind)
                } else {
                    Self::from_class(mapped_class)
                }
            }
            Protocol::Synthesized(synthesized) => Self::synthesized(
                synthesized.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
            Protocol::Materialized(materialized) => {
                if matches!(type_mapping, TypeMapping::Materialize(_)) {
                    self
                } else {
                    Self::materialized(
                        db,
                        materialized.origin(db).apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            visitor,
                        ),
                        materialized.materialization_kind(db),
                    )
                }
            }
        }
    }

    pub(super) fn interface(self, db: &'db dyn Db) -> ProtocolInterfaceView<'db> {
        self.inner.interface(db)
    }

    /// Returns constraints inferred from the nonrecursive requirements of `target`.
    ///
    /// Recursive requirements are omitted only while inferring a generic specialization. The
    /// eventual argument check must still compare against the complete protocol interface.
    pub(super) fn when_non_recursive_members_assignable_to_owned(
        self,
        db: &'db dyn Db,
        target: Self,
    ) -> Option<&'db OwnedConstraintSet<'db>> {
        let origin = target.class_origin(db)?;
        let interface = target.interface(db);
        let non_recursive = non_recursive_protocol_interface(
            db,
            interface.base(),
            origin,
            Type::ProtocolInstance(target),
        );
        let target = ProtocolInterfaceView::new(non_recursive, interface.materialization_kind());
        if target.member_count(db) == 0 {
            return None;
        }

        Some(non_recursive_protocol_constraints(db, self, target))
    }
}

#[salsa::tracked(attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _, ()| true, heap_size=ruff_memory_usage::heap_size)]
fn is_equivalent_to_object_inner<'db>(
    db: &'db dyn Db,
    protocol: ProtocolInstanceType<'db>,
    _: (),
) -> bool {
    match protocol_object_equivalence_sync(
        RelationFieldReads::new(db),
        protocol,
        &InlineProtocolObjectEffects::new(db),
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

#[cfg(test)]
pub(in crate::types) fn protocol_object_equivalence_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<
    impl salsa::plumbing::function::InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (ProtocolInstanceType<'a>, ())>
    + for<'a> salsa::plumbing::function::Configuration<DbView = dyn Db, Output<'a> = bool>,
> {
    is_equivalent_to_object_inner::fn_ingredient_(db, db.zalsa())
}

impl<'db> VarianceInferable<'db> for ProtocolInstanceType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        self.inner.variance_of(db, env, typevar)
    }
}

/// A class-backed protocol materialization whose member requirements remain lazy.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size, field_view=read_fields, field_requests=field_requests)]
pub(super) struct MaterializedProtocolType<'db> {
    #[returns(copy)]
    pub(super) origin: ProtocolClass<'db>,
    #[returns(copy)]
    pub(super) materialization_kind: MaterializationKind,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for MaterializedProtocolType<'_> {}

/// A class-backed, synthesized, or lazily materialized protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum Protocol<'db> {
    FromClass(ProtocolClass<'db>),
    Synthesized(SynthesizedProtocolType<'db>),
    Materialized(MaterializedProtocolType<'db>),
}

impl<'db> Protocol<'db> {
    /// Return the members of this protocol type
    fn interface(self, db: &'db dyn Db) -> ProtocolInterfaceView<'db> {
        match self {
            Self::FromClass(class) => ProtocolInterfaceView::new(class.interface(db), None),
            Self::Synthesized(synthesized) => {
                ProtocolInterfaceView::new(synthesized.interface(), None)
            }
            Self::Materialized(materialized) => ProtocolInterfaceView::new(
                materialized.origin(db).unmaterialized_interface(db),
                Some(materialized.materialization_kind(db)),
            ),
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::FromClass(class) => Some(Self::FromClass(
                class.recursive_type_normalized_impl(db, env, div, nested)?,
            )),
            Self::Synthesized(synthesized) => Some(Self::Synthesized(
                synthesized.recursive_type_normalized_impl(db, env, div, nested)?,
            )),
            Self::Materialized(materialized) => {
                Some(Self::Materialized(MaterializedProtocolType::new(
                    db,
                    materialized
                        .origin(db)
                        .recursive_type_normalized_impl(db, env, div, nested)?,
                    materialized.materialization_kind(db),
                )))
            }
        }
    }
}

impl<'db> VarianceInferable<'db> for Protocol<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        match self {
            Protocol::FromClass(class_type) => class_type.variance_of(db, env, typevar),
            Protocol::Synthesized(synthesized_protocol_type) => {
                synthesized_protocol_type.variance_of(db, env, typevar)
            }
            Protocol::Materialized(materialized) => {
                materialized.origin(db).variance_of(db, env, typevar)
            }
        }
    }
}

mod synthesized_protocol {

    use crate::types::protocol_class::ProtocolInterface;
    use crate::types::{
        ApplyTypeMappingVisitor, BoundTypeVarIdentity, Type, TypeContext, TypeMapping,
        VarianceInferable, VarianceTerm,
    };
    use crate::{Db, ProgramEnvironment};

    /// A "synthesized" protocol type that is dissociated from a class definition in source code.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
    pub(in crate::types) struct SynthesizedProtocolType<'db>(ProtocolInterface<'db>);

    impl<'db> SynthesizedProtocolType<'db> {
        pub(super) fn new(interface: ProtocolInterface<'db>) -> Self {
            Self(interface)
        }

        pub(super) fn apply_type_mapping_impl<'a>(
            self,
            db: &'db dyn Db,
            type_mapping: &TypeMapping<'a, 'db>,
            tcx: TypeContext<'db>,
            visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        ) -> Self {
            Self(
                self.0
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            )
        }

        pub(in crate::types) fn interface(self) -> ProtocolInterface<'db> {
            self.0
        }

        pub(in crate::types) fn recursive_type_normalized_impl(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            div: Type<'db>,
            nested: bool,
        ) -> Option<Self> {
            Some(Self(
                self.0
                    .recursive_type_normalized_impl(db, env, div, nested)?,
            ))
        }
    }

    impl<'db> VarianceInferable<'db> for SynthesizedProtocolType<'db> {
        fn variance_of(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            typevar: BoundTypeVarIdentity<'db>,
        ) -> VarianceTerm<'db> {
            self.0.variance_of(db, env, typevar)
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct NominalClassFacts;

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousNominalKnownClassEffects)]
pub(in crate::types) trait NominalKnownClassEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(source)]
    async fn explicit_any_class(&self, class: ExplicitAnyInstanceClass<'db>) -> Result<ClassType<'db>, Self::Error>;
    #[operation(source)]
    async fn generic_origin(&self, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Self::Error>;
    #[operation(source)]
    async fn static_known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
}
#[synchronous(SynchronousNominalGenericEffects)]
pub(in crate::types) trait NominalGenericEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn non_tuple_is_generic(&self, class: NominalInstanceClass<'db>) -> Result<bool, Self::Error>;
}
#[synchronous(SynchronousNominalClassEffects)]
pub(in crate::types) trait NominalClassEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> Result<ClassType<'db>, Self::Error>;
    #[operation(child)]
    async fn tuple_class(&self, tuple: TupleType<'db>) -> Result<ClassType<'db>, Self::Error>;
    #[operation(source)]
    async fn version_class(&self) -> Result<Option<ClassType<'db>>, Self::Error>;
    #[operation(source)]
    async fn object_class(&self) -> Result<ClassType<'db>, Self::Error>;
}
#[finite_capability]
impl NominalClassFacts {
    fn inner<'db>(&self, instance: NominalInstanceType<'db>) -> NominalInstanceInner<'db> { instance.0 }

}
#[synchronous(nominal_known_class_sync)]
#[capabilities(effects = NominalKnownClassEffects, facts = NominalClassFacts)]
#[passive_values(NominalInstanceInner::ExactTuple, NominalInstanceInner::NonTuple, NominalInstanceInner::SysVersionInfo, NominalInstanceInner::Object, NominalInstanceClass::Plain, NominalInstanceClass::InheritsFromExplicitAny, ClassType::NonGeneric, ClassType::Generic, ClassLiteral::Static, KnownClass::Tuple, KnownClass::VersionInfo, KnownClass::Object)]
pub(in crate::types) async fn nominal_known_class_with<'db, E: NominalKnownClassEffects<'db>>(
    instance: NominalInstanceType<'db>, facts: NominalClassFacts, effects: &E,
) -> Result<Option<KnownClass>, E::Error> {
    effects.checkpoint().await?;
    let class = match facts.inner(instance) {
        NominalInstanceInner::ExactTuple(_) => return Ok(Some(KnownClass::Tuple)),
        NominalInstanceInner::SysVersionInfo => return Ok(Some(KnownClass::VersionInfo)),
        NominalInstanceInner::Object => return Ok(Some(KnownClass::Object)),
        NominalInstanceInner::NonTuple(NominalInstanceClass::Plain(class)) => class,
        NominalInstanceInner::NonTuple(NominalInstanceClass::InheritsFromExplicitAny(class)) => effects.explicit_any_class(class).await?,
    };
    let literal = match class {
        ClassType::NonGeneric(ClassLiteral::Static(literal)) => literal,
        ClassType::Generic(alias) => effects.generic_origin(alias).await?,
        ClassType::NonGeneric(_) => return Ok(None),
    };
    effects.static_known(literal).await
}
#[synchronous(nominal_is_definition_generic_sync)]
#[capabilities(effects = NominalGenericEffects, facts = NominalClassFacts)]
#[passive_values(NominalInstanceInner::ExactTuple, NominalInstanceInner::NonTuple, NominalInstanceInner::SysVersionInfo, NominalInstanceInner::Object)]
pub(in crate::types) async fn nominal_is_definition_generic_with<'db, E: NominalGenericEffects<'db>>(
    instance: NominalInstanceType<'db>, facts: NominalClassFacts, effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint().await?;
    match facts.inner(instance) {
        NominalInstanceInner::ExactTuple(_) => Ok(true),
        NominalInstanceInner::SysVersionInfo | NominalInstanceInner::Object => Ok(false),
        NominalInstanceInner::NonTuple(class) => effects.non_tuple_is_generic(class).await,
    }
}
#[synchronous(nominal_class_sync)]
#[capabilities(effects = NominalClassEffects, facts = NominalClassFacts)]
#[passive_values(NominalInstanceInner::ExactTuple, NominalInstanceInner::NonTuple, NominalInstanceInner::SysVersionInfo, NominalInstanceInner::Object)]
pub(in crate::types) async fn nominal_class_with<'db, E: NominalClassEffects<'db>>(
    instance: NominalInstanceType<'db>, facts: NominalClassFacts, effects: &E,
) -> Result<ClassType<'db>, E::Error> {
    effects.checkpoint().await?;
    match facts.inner(instance) {
        NominalInstanceInner::ExactTuple(tuple) => effects.tuple_class(tuple).await,
        NominalInstanceInner::NonTuple(class) => effects.non_tuple_class(class).await,
        NominalInstanceInner::SysVersionInfo => match effects.version_class().await? {
            Some(class) => Ok(class),
            None => effects.object_class().await,
        },
        NominalInstanceInner::Object => effects.object_class().await,
    }
}
}

struct InlineNominalKnownClass<'db> {
    fields: salsa::FieldReads<'db>,
}

impl<'db> SynchronousNominalKnownClassEffects<'db> for InlineNominalKnownClass<'db> {
    type Error = std::convert::Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn explicit_any_class(
        &self,
        class: ExplicitAnyInstanceClass<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(*class.read_fields(self.fields).class())
    }

    fn generic_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(*alias.read_fields(self.fields).origin())
    }

    fn static_known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(*class.read_fields(self.fields).known())
    }
}

impl NominalClassFacts {
    pub(in crate::types) fn class<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        class: NominalInstanceClass<'db>,
    ) -> ClassType<'db> {
        match class {
            NominalInstanceClass::Plain(class) => class,
            NominalInstanceClass::InheritsFromExplicitAny(class) => {
                *class.read_fields(fields).class()
            }
        }
    }
}

struct InlineNominalClass<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}
impl<'db> SynchronousNominalClassEffects<'db> for InlineNominalClass<'_, 'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn non_tuple_class(
        &self,
        class: NominalInstanceClass<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(NominalClassFacts.class(salsa::FieldReads::new(self.db), class))
    }
    fn tuple_class(&self, tuple: TupleType<'db>) -> Result<ClassType<'db>, Self::Error> {
        Ok(tuple.to_class_type(self.db))
    }
    fn version_class(&self) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(sys_version_info_class(self.db, self.env))
    }
    fn object_class(&self) -> Result<ClassType<'db>, Self::Error> {
        Ok(ClassType::object(self.db, self.env))
    }
}

struct InlineNominalGeneric<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousNominalGenericEffects<'db> for InlineNominalGeneric<'db> {
    type Error = std::convert::Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn non_tuple_is_generic(&self, class: NominalInstanceClass<'db>) -> Result<bool, Self::Error> {
        Ok(NominalClassFacts
            .class(salsa::FieldReads::new(self.db), class)
            .is_generic())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::place::global_symbol;
    use crate::types::signatures::effects::try_poll_immediate;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Read<'db> {
        Checkpoint,
        ExplicitAny(ExplicitAnyInstanceClass<'db>),
        Origin(GenericAlias<'db>),
        Known(StaticClassLiteral<'db>),
    }

    struct ObservedKnownClass<'db> {
        ordinary: InlineNominalKnownClass<'db>,
        reads: RefCell<Vec<Read<'db>>>,
        refuse: Option<Read<'db>>,
    }

    impl<'db> ObservedKnownClass<'db> {
        fn record(&self, read: Read<'db>) -> Result<(), Read<'db>> {
            self.reads.borrow_mut().push(read);
            if self.refuse == Some(read) {
                Err(read)
            } else {
                Ok(())
            }
        }
    }

    impl<'db> NominalKnownClassEffects<'db> for ObservedKnownClass<'db> {
        type Error = Read<'db>;

        async fn checkpoint(&self) -> Result<(), Self::Error> {
            self.record(Read::Checkpoint)
        }

        async fn explicit_any_class(
            &self,
            class: ExplicitAnyInstanceClass<'db>,
        ) -> Result<ClassType<'db>, Self::Error> {
            self.record(Read::ExplicitAny(class))?;
            self.ordinary
                .explicit_any_class(class)
                .map_err(|never| match never {})
        }

        async fn generic_origin(
            &self,
            alias: GenericAlias<'db>,
        ) -> Result<StaticClassLiteral<'db>, Self::Error> {
            self.record(Read::Origin(alias))?;
            self.ordinary
                .generic_origin(alias)
                .map_err(|never| match never {})
        }

        async fn static_known(
            &self,
            class: StaticClassLiteral<'db>,
        ) -> Result<Option<KnownClass>, Self::Error> {
            self.record(Read::Known(class))?;
            self.ordinary
                .static_known(class)
                .map_err(|never| match never {})
        }
    }

    #[test]
    fn stored_known_class_preserves_forms_and_field_order() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/known_class.py",
                "class Plain: ...\nDynamic = type('Dynamic', (), {})\nitems: list[int]\n",
            )
            .build()?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/known_class.py")?,
            env.program(&db),
        );
        let symbol = |name| global_symbol(&db, file, name).place.expect_type();
        let Type::ClassLiteral(ClassLiteral::Static(plain)) = symbol("Plain") else {
            anyhow::bail!("Plain fixture must be a static class literal");
        };
        let Type::ClassLiteral(dynamic @ ClassLiteral::Dynamic(_)) = symbol("Dynamic") else {
            anyhow::bail!("Dynamic fixture must be a dynamic class literal");
        };
        let Type::NominalInstance(NominalInstanceType(NominalInstanceInner::NonTuple(
            NominalInstanceClass::Plain(ClassType::Generic(alias)),
        ))) = symbol("items")
        else {
            anyhow::bail!("items fixture must retain its generic class");
        };
        let origin = alias.origin(&db);
        let mut cases =
            vec![
                (
                    NominalInstanceType(NominalInstanceInner::ExactTuple(
                        TupleType::heterogeneous(&db, &env, []),
                    )),
                    Some(KnownClass::Tuple),
                    vec![],
                ),
                (
                    NominalInstanceType(NominalInstanceInner::SysVersionInfo),
                    Some(KnownClass::VersionInfo),
                    vec![],
                ),
                (
                    NominalInstanceType(NominalInstanceInner::Object),
                    Some(KnownClass::Object),
                    vec![],
                ),
            ];
        for (class, expected, reads) in [
            (
                ClassType::NonGeneric(ClassLiteral::Static(origin)),
                Some(KnownClass::List),
                vec![Read::Known(origin)],
            ),
            (
                ClassType::Generic(alias),
                Some(KnownClass::List),
                vec![Read::Origin(alias), Read::Known(origin)],
            ),
            (
                ClassType::NonGeneric(ClassLiteral::Static(plain)),
                None,
                vec![Read::Known(plain)],
            ),
            (ClassType::NonGeneric(dynamic), None, vec![]),
        ] {
            cases.push((
                NominalInstanceType(NominalInstanceInner::NonTuple(NominalInstanceClass::Plain(
                    class,
                ))),
                expected,
                reads.clone(),
            ));
            let wrapper = ExplicitAnyInstanceClass::new(&db, class);
            let mut wrapped_reads = vec![Read::ExplicitAny(wrapper)];
            wrapped_reads.extend(reads);
            cases.push((
                NominalInstanceType(NominalInstanceInner::NonTuple(
                    NominalInstanceClass::InheritsFromExplicitAny(wrapper),
                )),
                expected,
                wrapped_reads,
            ));
        }
        for (instance, expected, mut reads) in cases {
            reads.insert(0, Read::Checkpoint);
            let fields = salsa::FieldReads::new(&db);
            let mut effects = ObservedKnownClass {
                ordinary: InlineNominalKnownClass { fields },
                reads: RefCell::default(),
                refuse: None,
            };
            assert_eq!(instance.known_class(&db), expected);
            assert_eq!(instance.known_class_with_fields(fields), expected);
            assert_eq!(
                try_poll_immediate(nominal_known_class_with(
                    instance,
                    NominalClassFacts,
                    &effects
                )),
                Poll::Ready(Ok(expected)),
            );
            assert_eq!(*effects.reads.borrow(), reads);
            for (index, read) in reads.iter().copied().enumerate() {
                effects.refuse = Some(read);
                effects.reads.borrow_mut().clear();
                assert_eq!(
                    try_poll_immediate(nominal_known_class_with(
                        instance,
                        NominalClassFacts,
                        &effects
                    )),
                    Poll::Ready(Err(read)),
                );
                assert_eq!(*effects.reads.borrow(), reads[..=index]);
            }
        }
        Ok(())
    }
}
