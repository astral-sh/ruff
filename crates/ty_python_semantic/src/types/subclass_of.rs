use crate::Db;
use crate::ProgramEnvironment;
use self::metaclass::{
    OrdinaryMetaclassInstance, OrdinarySubclassMetaclass, SubclassMetaclassFacts,
    subclass_meta_type_sync, subclass_to_instance_sync,
};
use crate::place::PlaceAndQualifiers;
use crate::types::class::{DynamicClassLiteral, metaclass_instance_type};
use crate::types::constraints::ConstraintSet;
use crate::types::mapping::effects::{MappingEffects, MappingOperation};
use crate::types::member_lookup::mro_dispatch::{
    MroLookupFacts, OrdinaryMroLookupEffects, subclass_find_name_in_mro_sync,
};
use crate::types::relation::{DisjointnessChecker, TypeRelationChecker};
use crate::types::variance::{VarianceInferable, VarianceTerm};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, ClassLiteral, ClassType,
    DynamicType, IntersectionBuilder, KnownClass, MaterializationKind, MemberLookupPolicy,
    ProtocolInstanceType, SpecialFormType, Type, TypeContext, TypeMapping, TypeQualifiers,
    TypeRecursionContext, TypeVarBoundOrConstraints, TypedDictType, UnionBuilder, todo_type,
};

pub(in crate::types) mod metaclass;

/// A type that represents `type[C]`, i.e. the class object `C` and class objects that are subclasses of `C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct SubclassOfType<'db> {
    // Keep this field private, so that the only way of constructing the struct is through the `from` method.
    subclass_of: SubclassOfInner<'db>,
}

pub(super) fn walk_subclass_of_type<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    subclass_of: SubclassOfType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, Type::from(subclass_of));
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
    pub(crate) fn from(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        subclass_of: impl Into<SubclassOfInner<'db>>,
    ) -> Type<'db> {
        match subclass_from_sync(
            subclass_of.into(),
            SubclassConstructionFacts,
            &InlineSubclassConstruction { db, env },
        ) {
            Ok(ty) => ty,
            Err(error) => match error {},
        }
    }

    /// Construct the meta-type of a class-backed protocol.
    pub(super) const fn from_protocol(protocol: ProtocolInstanceType<'db>) -> Type<'db> {
        Type::SubclassOf(Self {
            subclass_of: SubclassOfInner::Protocol(protocol),
        })
    }

    /// Given the class object `T`, returns a [`Type`] instance representing `type[T]`.
    pub(crate) fn try_from_type(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Option<Type<'db>> {
        let subclass_of = match ty {
            Type::Dynamic(dynamic) => SubclassOfInner::Dynamic(dynamic),
            Type::ClassLiteral(literal) => {
                SubclassOfInner::Class(literal.default_specialization(db))
            }
            Type::GenericAlias(generic) => SubclassOfInner::Class(ClassType::Generic(generic)),
            Type::SpecialForm(SpecialFormType::Any) => SubclassOfInner::Dynamic(DynamicType::Any),
            Type::SpecialForm(SpecialFormType::Unknown) => {
                SubclassOfInner::Dynamic(DynamicType::Unknown)
            }
            _ => return None,
        };

        Some(Self::from(db, env, subclass_of))
    }

    /// Given an instance of the class or type variable `T`, returns a [`Type`] instance representing `type[T]`.
    /// Returns the unsupported component if conversion fails, including inside unions and intersections.
    pub(crate) fn try_from_instance(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Type<'db>> {
        match subclass_instance_sync(env, ty, &InlineSubclassInstance { db }) {
            Ok(result) => result,
            Err(error) => match error {},
        }
    }

    pub(in crate::types) async fn try_from_instance_with<E: SubclassInstanceEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        effects: &E,
    ) -> Result<Result<Type<'db>, Type<'db>>, E::Error> {
        let _ = db;
        subclass_instance_with(env, ty, effects).await
    }

    /// Return a [`Type`] instance representing the type `type[Unknown]`.
    pub(crate) const fn subclass_of_unknown() -> Type<'db> {
        Type::SubclassOf(SubclassOfType {
            subclass_of: SubclassOfInner::unknown(),
        })
    }

    /// Return a [`Type`] instance representing the type `type[Any]`.
    #[cfg(test)]
    pub(crate) const fn subclass_of_any() -> Type<'db> {
        Type::SubclassOf(SubclassOfType {
            subclass_of: SubclassOfInner::Dynamic(DynamicType::Any),
        })
    }

    /// Return a [`Type`] instance representing the type `type[object]`.
    fn subclass_of_object(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        // See the documentation of `SubclassOfType::from` for details.
        KnownClass::Type.to_instance(db, env)
    }

    /// Return the inner [`SubclassOfInner`] value wrapped by this `SubclassOfType`.
    pub(crate) const fn subclass_of(self) -> SubclassOfInner<'db> {
        self.subclass_of
    }

    /// Returns the effective write requirement exposed by `type[Protocol]` attribute lookup.
    pub(super) fn meta_write_requirement(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Option<(Option<Type<'db>>, TypeQualifiers)> {
        let SubclassOfInner::Protocol(protocol) = self.subclass_of else {
            return None;
        };
        protocol
            .interface(db)
            .meta_write_requirement(db, env, Type::ProtocolInstance(protocol), name)
            .map(|(write_ty, mut qualifiers)| {
                // `ClassVar` prohibits instance writes, not writes through the class object.
                qualifiers.remove(TypeQualifiers::CLASS_VAR);
                (write_ty, qualifiers)
            })
    }

    pub(crate) const fn is_dynamic(self) -> bool {
        // Unpack `self` so that we're forced to update this method if any more fields are added in the future.
        let Self { subclass_of } = self;
        subclass_of.is_dynamic()
    }

    pub(crate) const fn is_type_var(self) -> bool {
        let Self { subclass_of } = self;
        subclass_of.is_type_var()
    }

    pub const fn into_type_var(self) -> Option<BoundTypeVarInstance<'db>> {
        self.subclass_of.into_type_var()
    }

    /// Return the exact class-object type of this `type[T]` `TypeVar`'s upper bound, if it has one.
    ///
    /// This can only succeed when the upper bound normalizes to a final class.
    pub(crate) fn exact_typevar_upper_bound(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        self.into_type_var()
            .and_then(|typevar| typevar.typevar(db).upper_bound(db, env))
            .and_then(|bound| {
                let bound = Self::try_from_instance(db, env, bound.resolve_type_alias(db)).ok()?;
                matches!(bound, Type::ClassLiteral(_) | Type::GenericAlias(_)).then_some(bound)
            })
    }

    #[ty_mapping_probe_macros::dual_mapping]
    pub(super) async fn apply_type_mapping_with<'a, E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        Ok(match self.subclass_of {
            SubclassOfInner::Class(class) => Type::SubclassOf(Self {
                subclass_of: SubclassOfInner::Class(
                    class
                        .apply_type_mapping_with(db, type_mapping, tcx, visitor, effects)
                        .await?,
                ),
            }),
            SubclassOfInner::Protocol(protocol) => {
                effects.legacy(MappingOperation::Protocol, || {
                    protocol
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                        .to_meta_type(db, visitor.env)
                })?
            }
            SubclassOfInner::Dynamic(_) => match type_mapping {
                TypeMapping::Materialize(materialization_kind) => {
                    effects.legacy(MappingOperation::MaterializationOrPolarity, || {
                        match materialization_kind {
                            MaterializationKind::Top => {
                                KnownClass::Type.to_instance(db, visitor.env)
                            }
                            MaterializationKind::Bottom => Type::Never,
                        }
                    })?
                }
                _ => Type::SubclassOf(self),
            },
            SubclassOfInner::TypeVar(typevar) => {
                effects.legacy(MappingOperation::SubclassTypeVar, || {
                    let mapped = typevar.apply_type_mapping_impl(db, type_mapping, visitor);
                    Self::try_from_instance(db, visitor.env, mapped)
                        .unwrap_or_else(|_| visitor.project_meta_type(db, mapped))
                })?
            }
        })
    }

    pub(crate) fn find_name_in_mro_with_policy(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<PlaceAndQualifiers<'db>> {
        match subclass_find_name_in_mro_sync(
            self,
            env,
            name,
            policy,
            MroLookupFacts,
            &OrdinaryMroLookupEffects { db },
        ) {
            Ok(member) => member,
            Err(never) => match never {},
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            subclass_of: self
                .subclass_of
                .recursive_type_normalized_impl(db, env, div, nested)?,
        })
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match subclass_to_instance_sync(self.subclass_of, &OrdinaryMetaclassInstance { db, env }) {
            Ok(instance) => instance,
            Err(never) => match never {},
        }
    }

    /// Return a type representing "the set of all instances of the metaclass of this type".
    pub(crate) fn to_metaclass_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        // This kind of looks like a no-op, but it's not. For `type[C]` with guaranteed metaclass
        // `M`, `to_meta_type` produces `type[M]`, and then `to_instance` makes it just `M`.
        // And `to_meta_type` will transpose `type[T: C]` into `T: type[C]`, collapse to
        // the upper bound `type[C]`, and transform that to the meta-type `type[M]`, which
        // `to_instance` then resolves to `M`.
        metaclass_instance_type(db, env, self.to_meta_type(db, env))
    }

    /// Compute the metatype of this `type[T]`.
    ///
    /// For a concrete class `C`, this returns `type[M]`, where `M` is its guaranteed metaclass,
    /// excluding the lookup-only typeshed fallback.
    /// For `type[T]` where `T` is a `TypeVar`, this computes the metatype based on the
    /// `TypeVar`'s bounds or constraints.
    fn to_meta_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        Type::SubclassOf(self).to_meta_type(db, env)
    }

    pub(super) fn to_meta_type_with_recursion(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: &TypeRecursionContext<'db>,
    ) -> Type<'db> {
        match subclass_meta_type_sync(
            self.subclass_of,
            SubclassMetaclassFacts,
            &OrdinarySubclassMetaclass { db, env, context },
        ) {
            Ok(metaclass) => metaclass,
            Err(never) => match never {},
        }
    }

    pub(crate) fn is_typed_dict(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        self.subclass_of
            .into_class(db, env)
            .is_some_and(|class| class.class_literal(db).is_typed_dict(db))
    }
}

impl<'db> VarianceInferable<'db> for SubclassOfType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        match self.subclass_of {
            SubclassOfInner::Class(class) => class.variance_of(db, env, typevar),
            SubclassOfInner::Protocol(protocol) => protocol.variance_of(db, env, typevar),
            SubclassOfInner::TypeVar(inner) => Type::TypeVar(inner).variance_of(db, env, typevar),
            SubclassOfInner::Dynamic(_) => VarianceTerm::BIVARIANT,
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Return `true` if `source` has a certain relation to `other`.
    pub(crate) fn check_subclassof_pair(
        &self,
        db: &'db dyn Db,
        source: SubclassOfType<'db>,
        target: SubclassOfType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let SubclassOfInner::Protocol(target_protocol) = target.subclass_of {
            return self.check_meta_type_satisfies_protocol(
                db,
                Type::SubclassOf(source),
                target_protocol,
            );
        }
        if let SubclassOfInner::Protocol(source_protocol) = source.subclass_of {
            return self.check_type_pair(
                db,
                Type::ProtocolInstance(source_protocol),
                target.to_instance(db, self.env),
            );
        }

        match (source.subclass_of, target.subclass_of) {
            (SubclassOfInner::Dynamic(_), SubclassOfInner::Dynamic(_)) => {
                ConstraintSet::from_bool(self.constraints, !self.relation.is_subtyping())
            }
            (SubclassOfInner::Dynamic(_), SubclassOfInner::Class(target_class)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    target_class.is_object(db) || self.relation.is_assignability(),
                )
            }
            (SubclassOfInner::Class(_), SubclassOfInner::Dynamic(_)) => {
                ConstraintSet::from_bool(self.constraints, self.relation.is_assignability())
            }

            // For example, `type[bool]` describes all possible runtime subclasses of the class `bool`,
            // and `type[int]` describes all possible runtime subclasses of the class `int`.
            // The first set is a subset of the second set, because `bool` is itself a subclass of `int`.
            (SubclassOfInner::Class(source), SubclassOfInner::Class(target)) => {
                self.check_class_pair(db, source, target)
            }

            (SubclassOfInner::TypeVar(_), _) | (_, SubclassOfInner::TypeVar(_)) => {
                unreachable!()
            }
            (SubclassOfInner::Protocol(_), _) | (_, SubclassOfInner::Protocol(_)) => {
                unreachable!("protocol meta-types are handled above")
            }
        }
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    /// Return` true` if `left` is a disjoint type from `right`.
    ///
    /// See [`Type::is_disjoint_from`] for more details.
    pub(super) fn check_subclassof_pair(
        &self,
        db: &'db dyn Db,
        left: SubclassOfType<'db>,
        right: SubclassOfType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if matches!(left.subclass_of, SubclassOfInner::Protocol(_))
            || matches!(right.subclass_of, SubclassOfInner::Protocol(_))
        {
            // Protocols are open structural types, so their meta-types can generally overlap with
            // concrete class-object types and with other protocol meta-types.
            return ConstraintSet::from_bool(self.constraints, false);
        }

        match (left.subclass_of, right.subclass_of) {
            (SubclassOfInner::Dynamic(_), _) | (_, SubclassOfInner::Dynamic(_)) => {
                ConstraintSet::from_bool(self.constraints, false)
            }
            (SubclassOfInner::Class(left), SubclassOfInner::Class(right)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    !left.could_coexist_in_mro_with_disjointness_checker(db, self.env, right, self),
                )
            }
            (SubclassOfInner::TypeVar(_), _) | (_, SubclassOfInner::TypeVar(_)) => {
                unreachable!()
            }
            (SubclassOfInner::Protocol(_), _) | (_, SubclassOfInner::Protocol(_)) => {
                unreachable!("protocol meta-types are handled above")
            }
        }
    }
}

/// An enumeration of the different kinds of `type[]` types that a [`SubclassOfType`] can represent:
///
/// 1. A "subclass of a class": `type[C]` for any class object `C`
/// 2. A "subclass of a dynamic type": `type[Any]`, `type[Unknown]` and `type[@Todo]`
/// 3. A protocol meta-type: `type[P]` for a class-backed protocol `P`
/// 4. A "subclass of a type variable": `type[T]` for any type variable `T`
///
/// In the long term, we may want to implement <https://github.com/astral-sh/ruff/issues/15381>.
/// Doing this would allow us to get rid of this enum,
/// since `type[Any]` would be represented as `type & Any`
/// rather than using the [`Type::SubclassOf`] variant at all;
/// [`SubclassOfType`] would then be a simple wrapper around [`ClassType`].
///
/// Note that this enum is similar to the [`super::ClassBase`] enum, but does not include the
/// `ClassBase::Protocol` and `ClassBase::Generic` special-form variants (`type[Protocol]` and
/// `type[Generic]` are not valid types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum SubclassOfInner<'db> {
    Class(ClassType<'db>),
    Dynamic(DynamicType<'db>),
    Protocol(ProtocolInstanceType<'db>),
    TypeVar(BoundTypeVarInstance<'db>),
}

impl<'db> SubclassOfInner<'db> {
    const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    const fn is_dynamic(self) -> bool {
        matches!(self, Self::Dynamic(_))
    }

    const fn is_type_var(self) -> bool {
        matches!(self, Self::TypeVar(_))
    }

    pub(crate) fn into_class(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<ClassType<'db>> {
        match subclass_inner_into_class_sync(self, &InlineSubclassInnerClass { db, env }) {
            Ok(class) => class,
            Err(error) => match error {},
        }
    }

    pub(crate) const fn into_dynamic(self) -> Option<DynamicType<'db>> {
        match self {
            Self::Class(_) | Self::Protocol(_) | Self::TypeVar(_) => None,
            Self::Dynamic(dynamic) => Some(dynamic),
        }
    }

    pub(crate) const fn into_type_var(self) -> Option<BoundTypeVarInstance<'db>> {
        match self {
            Self::Class(_) | Self::Dynamic(_) | Self::Protocol(_) => None,
            Self::TypeVar(bound_typevar) => Some(bound_typevar),
        }
    }

    fn try_from_instance(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Option<Self> {
        match subclass_instance_inner_sync(
            env,
            ty,
            SubclassInstanceFacts,
            &InlineSubclassInstance { db },
        ) {
            Ok(inner) => inner,
            Err(error) => match error {},
        }
    }

    /// Converts `type[T]` with a type variable `T` into a type variable whose bound or
    /// constraints describe the runtime classes of `T`'s possible inhabitants.
    ///
    /// For ordinary nominal bounds, this looks like transposing `type[T]` into
    /// `T: type[...]`. The conversion intentionally goes through [`Type::to_meta_type`],
    /// though, so bounds such as function-like callables and custom metaclasses keep the
    /// richer meta-type that callers need instead of collapsing to `type[Unknown]`.
    ///
    /// In particular:
    /// - If `T` has an upper bound of `T: Bound`, this returns `T` with the meta-type of
    ///   `Bound` as its upper bound.
    /// - If `T` has constraints `T: (A, B)`, this returns `T` constrained by the meta-types
    ///   of `A` and `B`.
    /// - Otherwise, for an unbounded type variable, this returns `type[object]`.
    ///
    /// If this is type of a concrete type `C`, returns the type unchanged.
    pub(crate) fn with_transposed_type_var(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        self.with_transposed_type_var_with_recursion(db, env, &TypeRecursionContext::default())
    }

    fn with_transposed_type_var_with_recursion(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: &TypeRecursionContext<'db>,
    ) -> Self {
        let Some(bound_typevar) = self.into_type_var() else {
            return self;
        };

        let bound_typevar = bound_typevar.map_bound_or_constraints(db, |bound_or_constraints| {
            Some(match bound_or_constraints {
                None => TypeVarBoundOrConstraints::UpperBound(
                    SubclassOfType::try_from_instance(db, env, bound_typevar.domain(db).top(db))
                        .unwrap_or(SubclassOfType::subclass_of_unknown()),
                ),
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    TypeVarBoundOrConstraints::UpperBound(
                        bound.to_meta_type_with_recursion(db, env, context),
                    )
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    TypeVarBoundOrConstraints::Constraints(constraints.map(db, |constraint| {
                        constraint.to_meta_type_with_recursion(db, env, context)
                    }))
                }
            })
        });

        Self::TypeVar(bound_typevar)
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Class(class) => Some(Self::Class(
                class.recursive_type_normalized_impl(db, env, div, nested)?,
            )),
            Self::Dynamic(dynamic) => Some(Self::Dynamic(dynamic.recursive_type_normalized())),
            Self::Protocol(protocol) => Some(Self::Protocol(
                protocol.recursive_type_normalized_impl(db, env, div, nested)?,
            )),
            Self::TypeVar(_) => Some(self),
        }
    }
}

impl<'db> From<ClassType<'db>> for SubclassOfInner<'db> {
    fn from(value: ClassType<'db>) -> Self {
        SubclassOfInner::Class(value)
    }
}

impl<'db> From<DynamicType<'db>> for SubclassOfInner<'db> {
    fn from(value: DynamicType<'db>) -> Self {
        SubclassOfInner::Dynamic(value)
    }
}

impl<'db> From<BoundTypeVarInstance<'db>> for SubclassOfInner<'db> {
    fn from(value: BoundTypeVarInstance<'db>) -> Self {
        SubclassOfInner::TypeVar(value)
    }
}

impl<'db> From<SubclassOfType<'db>> for Type<'db> {
    fn from(value: SubclassOfType<'db>) -> Self {
        match value.subclass_of {
            SubclassOfInner::Class(class) => class.into(),
            SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SubclassOfInner::Protocol(protocol) => Type::ProtocolInstance(protocol),
            SubclassOfInner::TypeVar(bound_typevar) => Type::TypeVar(bound_typevar),
        }
    }
}

impl<'db> From<DynamicClassLiteral<'db>> for SubclassOfInner<'db> {
    fn from(value: DynamicClassLiteral<'db>) -> Self {
        SubclassOfInner::Class(ClassType::NonGeneric(ClassLiteral::Dynamic(value)))
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSubclassInnerClassEffects)]
    pub(in crate::types) trait SubclassInnerClassEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn require_bound_or_constraints(
            &self,
            typevar: BoundTypeVarInstance<'db>,
        ) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error>;
        #[operation(child)]
        async fn bound_into_class(&self, bound: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn object_class(&self) -> Result<ClassType<'db>, Self::Error>;
    }

    #[synchronous(subclass_inner_into_class_sync)]
    #[capabilities(effects = SubclassInnerClassEffects)]
    #[passive_values()]
    pub(in crate::types) async fn subclass_inner_into_class_with<'db, E: SubclassInnerClassEffects<'db>>(
        subclass_of: SubclassOfInner<'db>,
        effects: &E,
    ) -> Result<Option<ClassType<'db>>, E::Error> {
        effects.checkpoint().await?;
        match subclass_of {
            SubclassOfInner::Dynamic(_) | SubclassOfInner::Protocol(_) => Ok(None),
            SubclassOfInner::Class(class) => Ok(Some(class)),
            SubclassOfInner::TypeVar(typevar) => {
                match effects.require_bound_or_constraints(typevar).await? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => effects.bound_into_class(bound).await,
                    // TODO this is quite imprecise
                    TypeVarBoundOrConstraints::Constraints(_) => Ok(Some(effects.object_class().await?)),
                }
            }
        }
    }
}

struct InlineSubclassInnerClass<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousSubclassInnerClassEffects<'db> for InlineSubclassInnerClass<'_, 'db> {
    type Error = std::convert::Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn require_bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error> {
        Ok(typevar.require_bound_or_constraints(self.db, self.env))
    }

    fn bound_into_class(&self, bound: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(SubclassOfInner::try_from_instance(self.db, self.env, bound)
            .and_then(|subclass_of| subclass_of.into_class(self.db, self.env)))
    }

    fn object_class(&self) -> Result<ClassType<'db>, Self::Error> {
        Ok(ClassType::object(self.db, self.env))
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct SubclassConstructionFacts;

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousSubclassConstructionEffects)]
pub(in crate::types) trait SubclassConstructionEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn is_final(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;
    #[operation(source)]
    async fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;
    #[operation(child)]
    async fn subclass_of_object(&self) -> Result<Type<'db>, Self::Error>;
}

#[finite_capability]
impl SubclassConstructionFacts {
    fn class_type<'db>(&self, class: ClassType<'db>) -> Type<'db> {
        Type::from(class)
    }
    fn subclass<'db>(&self, subclass_of: SubclassOfInner<'db>) -> Type<'db> {
        Type::SubclassOf(SubclassOfType { subclass_of })
    }
}

#[synchronous(subclass_from_sync)]
#[capabilities(effects = SubclassConstructionEffects, facts = SubclassConstructionFacts)]
#[passive_values(SubclassOfInner::Class, SubclassOfInner::Dynamic, SubclassOfInner::Protocol, SubclassOfInner::TypeVar)]
pub(in crate::types) async fn subclass_from_with<'db, E: SubclassConstructionEffects<'db>>(
    subclass_of: SubclassOfInner<'db>,
    facts: SubclassConstructionFacts,
    effects: &E,
) -> Result<Type<'db>, E::Error> {
    effects.checkpoint().await?;
    match subclass_of {
        SubclassOfInner::Class(class) => {
            if effects.is_final(class).await? {
                Ok(facts.class_type(class))
            } else if effects.is_object(class).await? {
                effects.subclass_of_object().await
            } else {
                Ok(facts.subclass(subclass_of))
            }
        }
        SubclassOfInner::Dynamic(_) | SubclassOfInner::Protocol(_) | SubclassOfInner::TypeVar(_) => Ok(facts.subclass(subclass_of)),
    }
}
}

struct InlineSubclassConstruction<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousSubclassConstructionEffects<'db> for InlineSubclassConstruction<'_, 'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn is_final(&self, class: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_final(self.db))
    }
    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_object(self.db))
    }
    fn subclass_of_object(&self) -> Result<Type<'db>, Self::Error> {
        Ok(SubclassOfType::subclass_of_object(self.db, self.env))
    }
}

pub(in crate::types) struct SubclassInstanceFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSubclassInstanceEffects)]
    pub(in crate::types) trait SubclassInstanceEffects<'db> {
        type Error;
        #[operation(source)]
        async fn nominal_class(&self, env: &ProgramEnvironment<'db>, instance: crate::types::NominalInstanceType<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(source)]
        async fn negative_empty(&self, intersection: crate::types::IntersectionType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn union_conversion(&self, env: &ProgramEnvironment<'db>, union: crate::types::UnionType<'db>) -> Result<Result<Type<'db>, Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn intersection_conversion(&self, env: &ProgramEnvironment<'db>, intersection: crate::types::IntersectionType<'db>) -> Result<Result<Type<'db>, Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn protocol_meta(&self, env: &ProgramEnvironment<'db>, protocol: ProtocolInstanceType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn inner(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<Option<SubclassOfInner<'db>>, Self::Error>;
        #[operation(child)]
        async fn subclass(&self, env: &ProgramEnvironment<'db>, inner: SubclassOfInner<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl SubclassInstanceFacts {
        fn typed_dict<'db>(&self, typed_dict: TypedDictType<'db>) -> SubclassOfInner<'db> {
            match typed_dict {
                TypedDictType::Class(class) => SubclassOfInner::Class(class),
                TypedDictType::Synthesized(_) => SubclassOfInner::Dynamic(
                    todo_type!("type[T] for synthesized TypedDicts").expect_dynamic(),
                ),
            }
        }
    }

    #[synchronous(subclass_instance_inner_sync)]
    #[capabilities(effects = SubclassInstanceEffects, facts = SubclassInstanceFacts)]
    #[passive_values(SubclassOfInner::Class, SubclassOfInner::TypeVar, SubclassOfInner::Dynamic, DynamicType::Any, DynamicType::Unknown)]
    pub(in crate::types) async fn subclass_instance_inner_with<'db, E: SubclassInstanceEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, facts: SubclassInstanceFacts, effects: &E,
    ) -> Result<Option<SubclassOfInner<'db>>, E::Error> {
        let inner = match ty {
            Type::NominalInstance(instance) => SubclassOfInner::Class(effects.nominal_class(env, instance).await?),
            Type::TypedDict(typed_dict) => facts.typed_dict(typed_dict),
            Type::TypeVar(typevar) => SubclassOfInner::TypeVar(typevar),
            Type::Dynamic(DynamicType::Any) => SubclassOfInner::Dynamic(DynamicType::Any),
            Type::Dynamic(DynamicType::Unknown) => SubclassOfInner::Dynamic(DynamicType::Unknown),
            _ => return Ok(None),
        };
        Ok(Some(inner))
    }

    #[synchronous(subclass_instance_sync)]
    #[capabilities(effects = SubclassInstanceEffects)]
    #[passive_values(Type::Never, Err)]
    pub(in crate::types) async fn subclass_instance_with<'db, E: SubclassInstanceEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, effects: &E,
    ) -> Result<Result<Type<'db>, Type<'db>>, E::Error> {
        // Handle unions and intersections by distributing `type[]` over each element:
        // `type[A | B]` -> `type[A] | type[B]`
        // `type[A & B]` -> `type[A] & type[B]`
        match ty {
            Type::Never => return Ok(Ok(Type::Never)),
            Type::Union(union) => return effects.union_conversion(env, union).await,
            Type::Intersection(intersection) if effects.negative_empty(intersection).await? => return effects.intersection_conversion(env, intersection).await,
            Type::ProtocolInstance(protocol) => return Ok(Ok(effects.protocol_meta(env, protocol).await?)),
            _ => {}
        }
        match effects.inner(env, ty).await? {
            Some(inner) => Ok(Ok(effects.subclass(env, inner).await?)),
            None => Ok(Err(ty)),
        }
    }
}

struct InlineSubclassInstance<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousSubclassInstanceEffects<'db> for InlineSubclassInstance<'db> {
    type Error = std::convert::Infallible;
    fn nominal_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: crate::types::NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(instance.class(self.db, env))
    }
    fn negative_empty(
        &self,
        intersection: crate::types::IntersectionType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(intersection.negative(self.db).is_empty())
    }
    fn union_conversion(
        &self,
        env: &ProgramEnvironment<'db>,
        union: crate::types::UnionType<'db>,
    ) -> Result<Result<Type<'db>, Type<'db>>, Self::Error> {
        Ok(union
            .elements(self.db)
            .iter()
            .try_fold(UnionBuilder::new(self.db, env), |builder, element| {
                Ok(builder.add(SubclassOfType::try_from_instance(self.db, env, *element)?))
            })
            .map(UnionBuilder::build))
    }
    fn intersection_conversion(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: crate::types::IntersectionType<'db>,
    ) -> Result<Result<Type<'db>, Type<'db>>, Self::Error> {
        Ok(intersection
            .iter_positive(self.db)
            .try_fold(
                IntersectionBuilder::new(self.db, env),
                |builder, element| {
                    Ok(builder
                        .add_positive(SubclassOfType::try_from_instance(self.db, env, element)?))
                },
            )
            .map(IntersectionBuilder::build))
    }
    fn protocol_meta(
        &self,
        env: &ProgramEnvironment<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(protocol.to_meta_type(self.db, env))
    }
    fn inner(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<SubclassOfInner<'db>>, Self::Error> {
        subclass_instance_inner_sync(env, ty, SubclassInstanceFacts, self)
    }
    fn subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        inner: SubclassOfInner<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(SubclassOfType::from(self.db, env, inner))
    }
}
