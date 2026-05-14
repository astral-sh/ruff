use crate::place::PlaceAndQualifiers;
use crate::types::class::DynamicClassLiteral;
use crate::types::constraints::ConstraintSet;
use crate::types::protocol_class::ProtocolClass;
use crate::types::relation::{DisjointnessChecker, TypeRelationChecker};
use crate::types::variance::VarianceInferable;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ClassLiteral, ClassType, DynamicType,
    FindLegacyTypeVarsVisitor, KnownClass, MaterializationKind, MemberLookupPolicy,
    SpecialFormType, Type, TypeContext, TypeMapping, TypeVarBoundOrConstraints, TypeVarVariance,
    TypedDictType, UnionType, todo_type,
};
use crate::{Db, FxOrderSet};
use ty_python_core::definition::Definition;

/// A type that represents `type[C]`, i.e. the class object `C` and class objects that are subclasses of `C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
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
    pub(crate) fn from(db: &'db dyn Db, subclass_of: impl Into<SubclassOfInner<'db>>) -> Type<'db> {
        let subclass_of = subclass_of.into();
        match subclass_of {
            SubclassOfInner::Class(class) => {
                if class.is_final(db) {
                    Type::from(class)
                } else if class.is_object(db) {
                    Self::subclass_of_object(db)
                } else {
                    Type::SubclassOf(Self { subclass_of })
                }
            }
            SubclassOfInner::Dynamic(_) | SubclassOfInner::TypeVar(_) => {
                Type::SubclassOf(Self { subclass_of })
            }
        }
    }

    /// Given the class object `T`, returns a [`Type`] instance representing `type[T]`.
    pub(crate) fn try_from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Type<'db>> {
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

        Some(Self::from(db, subclass_of))
    }

    /// Given an instance of the class or type variable `T`, returns a [`Type`] instance representing `type[T]`.
    pub(crate) fn try_from_instance(db: &'db dyn Db, ty: Type<'db>) -> Option<Type<'db>> {
        // Handle unions by distributing `type[]` over each element:
        // `type[A | B]` -> `type[A] | type[B]`
        match ty {
            Type::Union(union) => UnionType::try_from_elements(
                db,
                union
                    .elements(db)
                    .iter()
                    .map(|element| Self::try_from_instance(db, *element)),
            ),
            Type::ProtocolInstance(protocol) => Some(protocol.to_meta_type(db)),
            _ => SubclassOfInner::try_from_instance(db, ty)
                .map(|subclass_of| Self::from(db, subclass_of)),
        }
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
    pub(crate) fn subclass_of_object(db: &'db dyn Db) -> Type<'db> {
        // See the documentation of `SubclassOfType::from` for details.
        KnownClass::Type.to_instance(db)
    }

    /// Return the inner [`SubclassOfInner`] value wrapped by this `SubclassOfType`.
    pub(crate) const fn subclass_of(self) -> SubclassOfInner<'db> {
        self.subclass_of
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

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        match self.subclass_of {
            SubclassOfInner::Class(class) => Type::SubclassOf(Self {
                subclass_of: SubclassOfInner::Class(class.apply_type_mapping_impl(
                    db,
                    type_mapping,
                    tcx,
                    visitor,
                )),
            }),
            SubclassOfInner::Dynamic(_) => match type_mapping {
                TypeMapping::Materialize(materialization_kind) => match materialization_kind {
                    MaterializationKind::Top => KnownClass::Type.to_instance(db),
                    MaterializationKind::Bottom => Type::Never,
                },
                _ => Type::SubclassOf(self),
            },
            SubclassOfInner::TypeVar(typevar) => {
                let mapped = typevar.apply_type_mapping_impl(db, type_mapping, visitor);
                mapped.to_meta_type(db)
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
        match self.subclass_of {
            SubclassOfInner::Dynamic(_) => {}
            SubclassOfInner::Class(class) => {
                class.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            SubclassOfInner::TypeVar(typevar) => {
                Type::TypeVar(typevar).find_legacy_typevars_impl(
                    db,
                    binding_context,
                    typevars,
                    visitor,
                );
            }
        }
    }

    pub(crate) fn find_name_in_mro_with_policy(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<PlaceAndQualifiers<'db>> {
        let class_like = match self.subclass_of.with_transposed_type_var(db) {
            SubclassOfInner::Class(class) => Type::from(class),
            SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SubclassOfInner::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound,
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.as_type(db)
                    }
                }
            }
        };

        class_like.find_name_in_mro_with_policy(db, name, policy)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            subclass_of: self
                .subclass_of
                .recursive_type_normalized_impl(db, div, nested)?,
        })
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        match self.subclass_of {
            SubclassOfInner::Class(class) => Type::instance(db, class),
            SubclassOfInner::Dynamic(dynamic_type) => Type::Dynamic(dynamic_type),
            SubclassOfInner::TypeVar(bound_typevar) => Type::TypeVar(bound_typevar),
        }
    }

    /// Return a type representing "the set of all instances of the metaclass of this type".
    pub(crate) fn to_metaclass_instance(self, db: &'db dyn Db) -> Type<'db> {
        // This kind of looks like a no-op, but it's not. For `type[C]` where `C` has metaclass
        // `M`, `to_meta_type` transforms `type[C]` to `type[M]`, and then `to_instance` makes it
        // just `M`. And `to_meta_type` will transpose `type[T: C]` into `T: type[C]`, collapse to
        // the upper bound `type[C]`, and transform that to the meta-type `type[M]`, which
        // `to_instance` then resolves to `M`.
        self.to_meta_type(db)
            .to_instance(db)
            .expect("the meta-type of a SubclassOf type should always be instantiable")
    }

    /// Compute the metatype of this `type[T]`.
    ///
    /// For `type[C]` where `C` is a concrete class, this returns `type[metaclass(C)]`.
    /// For `type[T]` where `T` is a `TypeVar`, this computes the metatype based on the
    /// `TypeVar`'s bounds or constraints.
    pub(crate) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        match self.subclass_of.with_transposed_type_var(db) {
            SubclassOfInner::Dynamic(dynamic) => {
                SubclassOfType::from(db, SubclassOfInner::Dynamic(dynamic))
            }
            SubclassOfInner::Class(class) => SubclassOfType::try_from_type(db, class.metaclass(db))
                .unwrap_or(SubclassOfType::subclass_of_unknown()),
            // For `type[T]` where `T` is a TypeVar, `with_transposed_type_var` transforms
            // the bounds from instance types to `type[]` types. For example, `type[T]` where
            // `T: A | B` becomes a TypeVar with bound `type[A] | type[B]`. The metatype is
            // then the metatype of that bound.
            SubclassOfInner::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    // `with_transposed_type_var` always adds a bound for unbounded TypeVars
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.to_meta_type(db),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.as_type(db).to_meta_type(db)
                    }
                }
            }
        }
    }

    pub(crate) fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        self.subclass_of
            .into_class(db)
            .is_some_and(|class| class.class_literal(db).is_typed_dict(db))
    }
}

impl<'db> VarianceInferable<'db> for SubclassOfType<'db> {
    fn variance_of(self, db: &dyn Db, typevar: BoundTypeVarInstance<'_>) -> TypeVarVariance {
        match self.subclass_of {
            SubclassOfInner::Class(class) => class.variance_of(db, typevar),
            SubclassOfInner::Dynamic(_) | SubclassOfInner::TypeVar(_) => TypeVarVariance::Bivariant,
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
        match (left.subclass_of, right.subclass_of) {
            (SubclassOfInner::Dynamic(_), _) | (_, SubclassOfInner::Dynamic(_)) => {
                ConstraintSet::from_bool(self.constraints, false)
            }
            (SubclassOfInner::Class(left), SubclassOfInner::Class(right)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    !left.could_coexist_in_mro_with(db, right, self.constraints),
                )
            }
            (SubclassOfInner::TypeVar(_), _) | (_, SubclassOfInner::TypeVar(_)) => {
                unreachable!()
            }
        }
    }
}

/// An enumeration of the different kinds of `type[]` types that a [`SubclassOfType`] can represent:
///
/// 1. A "subclass of a class": `type[C]` for any class object `C`
/// 2. A "subclass of a dynamic type": `type[Any]`, `type[Unknown]` and `type[@Todo]`
/// 3. A "subclass of a type variable": `type[T]` for any type variable `T`
///
/// In the long term, we may want to implement <https://github.com/astral-sh/ruff/issues/15381>.
/// Doing this would allow us to get rid of this enum,
/// since `type[Any]` would be represented as `type & Any`
/// rather than using the [`Type::SubclassOf`] variant at all;
/// [`SubclassOfType`] would then be a simple wrapper around [`ClassType`].
///
/// Note that this enum is similar to the [`super::ClassBase`] enum,
/// but does not include the `ClassBase::Protocol` and `ClassBase::Generic` variants
/// (`type[Protocol]` and `type[Generic]` are not valid types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum SubclassOfInner<'db> {
    Class(ClassType<'db>),
    Dynamic(DynamicType<'db>),
    TypeVar(BoundTypeVarInstance<'db>),
}

impl<'db> SubclassOfInner<'db> {
    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) const fn is_dynamic(self) -> bool {
        matches!(self, Self::Dynamic(_))
    }

    pub(crate) const fn is_type_var(self) -> bool {
        matches!(self, Self::TypeVar(_))
    }

    pub(crate) fn into_class(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        match self {
            Self::Dynamic(_) => None,
            Self::Class(class) => Some(class),
            Self::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => Some(ClassType::object(db)),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        Self::try_from_instance(db, bound)
                            .and_then(|subclass_of| subclass_of.into_class(db))
                    }
                    // TODO this is quite imprecise
                    Some(TypeVarBoundOrConstraints::Constraints(_)) => Some(ClassType::object(db)),
                }
            }
        }
    }

    pub(crate) const fn into_dynamic(self) -> Option<DynamicType<'db>> {
        match self {
            Self::Class(_) | Self::TypeVar(_) => None,
            Self::Dynamic(dynamic) => Some(dynamic),
        }
    }

    pub(crate) const fn into_type_var(self) -> Option<BoundTypeVarInstance<'db>> {
        match self {
            Self::Class(_) | Self::Dynamic(_) => None,
            Self::TypeVar(bound_typevar) => Some(bound_typevar),
        }
    }

    pub(crate) fn try_from_instance(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        Some(match ty {
            Type::NominalInstance(instance) => SubclassOfInner::Class(instance.class(db)),
            Type::TypedDict(typed_dict) => match typed_dict {
                TypedDictType::Class(class) => SubclassOfInner::Class(class),
                TypedDictType::Synthesized(_) => SubclassOfInner::Dynamic(
                    todo_type!("type[T] for synthesized TypedDicts").expect_dynamic(),
                ),
            },
            Type::TypeVar(bound_typevar) => SubclassOfInner::TypeVar(bound_typevar),
            Type::Dynamic(DynamicType::Any) => SubclassOfInner::Dynamic(DynamicType::Any),
            Type::Dynamic(DynamicType::Unknown) => SubclassOfInner::Dynamic(DynamicType::Unknown),
            _ => return None,
        })
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
    pub(crate) fn with_transposed_type_var(self, db: &'db dyn Db) -> Self {
        let Some(bound_typevar) = self.into_type_var() else {
            return self;
        };

        let bound_typevar = bound_typevar.map_bound_or_constraints(db, |bound_or_constraints| {
            Some(match bound_or_constraints {
                None => TypeVarBoundOrConstraints::UpperBound(
                    SubclassOfType::try_from_instance(db, Type::object())
                        .unwrap_or(SubclassOfType::subclass_of_unknown()),
                ),
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    TypeVarBoundOrConstraints::UpperBound(bound.to_meta_type(db))
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    TypeVarBoundOrConstraints::Constraints(
                        constraints.map(db, |constraint| constraint.to_meta_type(db)),
                    )
                }
            })
        });

        Self::TypeVar(bound_typevar)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::Class(class) => Some(Self::Class(
                class.recursive_type_normalized_impl(db, div, nested)?,
            )),
            Self::Dynamic(dynamic) => Some(Self::Dynamic(dynamic.recursive_type_normalized())),
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

impl<'db> From<ProtocolClass<'db>> for SubclassOfInner<'db> {
    fn from(value: ProtocolClass<'db>) -> Self {
        SubclassOfInner::Class(*value)
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
            SubclassOfInner::TypeVar(bound_typevar) => Type::TypeVar(bound_typevar),
        }
    }
}

impl<'db> From<DynamicClassLiteral<'db>> for SubclassOfInner<'db> {
    fn from(value: DynamicClassLiteral<'db>) -> Self {
        SubclassOfInner::Class(ClassType::NonGeneric(ClassLiteral::Dynamic(value)))
    }
}
