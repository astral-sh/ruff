use crate::Db;
use crate::types::generics::{Specialization, specialization_variance};
use crate::types::tuple::FixedLengthTuple;
use crate::types::{
    IntersectionType, KnownClass, StaticClassLiteral, Type, TypeVarVariance, UnionType,
};

/// Simplify the intersection of two generic specializations when one is a gradual
/// generalization of the other.
///
/// For example, `list[int] & list[Any]` simplifies to `list[int]`, while
/// `Sequence[int] & Sequence[Any]` simplifies to `Sequence[int & Any]`.
pub(super) fn generic_gradual_intersection<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> Option<Type<'db>> {
    dynamic_generalization_of(db, left, right)
        .or_else(|| dynamic_generalization_of(db, right, left))
        .map(|generalization| generalization.intersection(db))
}

/// If `general` is a dynamic generalization of the fully-static `specific`, return the bottom
/// materialization that should replace `general` in `specific & ~general`.
///
/// For example, `Co[P] & ~Co[Any] = Co[P] & ~Bottom[Co[Any]] & Any`; this function returns
/// `Bottom[Co[Any]]`. Similarly, `type[P] & ~type[Any] = type[P] & ~type[Never] & Any`; this
/// function returns `type[Never]`.
pub(super) fn negated_generalization_bottom<'db>(
    db: &'db dyn Db,
    general: Type<'db>,
    specific: Type<'db>,
) -> Option<Type<'db>> {
    if let (Type::SubclassOf(general_subclass), Type::SubclassOf(_)) = (general, specific)
        && general_subclass.is_dynamic()
        && !specific.has_dynamic(db)
    {
        return Some(general.bottom_materialization(db));
    }

    dynamic_generalization_of(db, general, specific)?;
    if specific.has_dynamic(db) {
        None
    } else {
        Some(general.bottom_materialization(db))
    }
}

/// Describes how intersecting a dynamic generic specialization such as `list[Any]` with a more
/// specific specialization such as `list[int]` should be simplified.
enum DynamicGeneralization<'db> {
    /// All dynamic replacements occur in invariant or bivariant positions, so the intersection
    /// simplifies to the original specific type.
    ///
    /// For example, `list[int] & list[Any] = list[int]`.
    UseSpecific(Type<'db>),
    /// At least one dynamic replacement occurs in a covariant or contravariant position, so the
    /// intersection requires rebuilding the specialization according to each parameter's variance.
    ///
    /// For example, `Sequence[int] & Sequence[Any] = Sequence[int & Any]`.
    RebuildSpecialization {
        class: StaticClassLiteral<'db>,
        general: Specialization<'db>,
        specific: Specialization<'db>,
    },
    /// Tuple specializations require rebuilding each element independently.
    RebuildTuple {
        general: &'db FixedLengthTuple<Type<'db>>,
        specific: &'db FixedLengthTuple<Type<'db>>,
    },
}

impl<'db> DynamicGeneralization<'db> {
    /// Simplify the intersection represented by this relationship.
    ///
    /// Returns the original specific type when no reconstruction is needed, or rebuilds the
    /// specialization according to its parameter variances.
    fn intersection(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::UseSpecific(specific) => specific,
            Self::RebuildTuple { general, specific } => Type::heterogeneous_tuple(
                db,
                specific
                    .iter_all_elements()
                    .zip(general.iter_all_elements())
                    .map(|(specific, general)| {
                        IntersectionType::from_two_elements(db, specific, general)
                    }),
            ),
            Self::RebuildSpecialization {
                class,
                general,
                specific,
            } => {
                let generic_context = general.generic_context(db);
                let types: Vec<_> = generic_context
                    .variables(db)
                    .zip(general.types(db))
                    .zip(specific.types(db))
                    .map(|((typevar, general), specific)| {
                        if general == specific {
                            return *specific;
                        }
                        match specialization_variance(db, typevar) {
                            TypeVarVariance::Covariant => {
                                IntersectionType::from_two_elements(db, *specific, *general)
                            }
                            TypeVarVariance::Contravariant => {
                                UnionType::from_two_elements(db, *specific, *general)
                            }
                            TypeVarVariance::Invariant | TypeVarVariance::Bivariant => *specific,
                        }
                    })
                    .collect();
                let specialization = generic_context.specialize(db, types);

                Type::instance(
                    db,
                    class.apply_optional_specialization(db, Some(specialization)),
                )
            }
        }
    }
}

/// Return the relationship between two specializations of the same generic class if `general`
/// only differs from `specific` by using dynamic types.
///
/// For example, `list[Any]` dynamically generalizes `list[int]`, while `list[str]` does not.
fn dynamic_generalization_of<'db>(
    db: &'db dyn Db,
    general: Type<'db>,
    specific: Type<'db>,
) -> Option<DynamicGeneralization<'db>> {
    // Fast path to avoid performance regressions.
    if !general.has_dynamic(db)
        || matches!(general, Type::TypeVar(_) | Type::NewTypeInstance(_))
        || matches!(specific, Type::TypeVar(_) | Type::NewTypeInstance(_))
    {
        return None;
    }

    let (
        Some((general_class, general_specialization)),
        Some((specific_class, specific_specialization)),
    ) = (
        general.class_specialization(db),
        specific.class_specialization(db),
    )
    else {
        return None;
    };

    // Top and bottom materializations are not gradual types.
    if general_class != specific_class
        || general_specialization == specific_specialization
        || general_specialization.materialization_kind(db).is_some()
        || specific_specialization.materialization_kind(db).is_some()
    {
        return None;
    }

    if general_class.known(db) == Some(KnownClass::Tuple) {
        let general_tuple = general_specialization.tuple(db)?.as_fixed_length()?;
        let specific_tuple = specific_specialization.tuple(db)?.as_fixed_length()?;
        if general_tuple.len() != specific_tuple.len()
            || general_tuple
                .iter_all_elements()
                .zip(specific_tuple.iter_all_elements())
                .any(|(general, specific)| {
                    general != specific && !general.is_non_divergent_dynamic()
                })
        {
            return None;
        }
        if specific.has_dynamic(db) {
            return None;
        }

        return Some(DynamicGeneralization::RebuildTuple {
            general: general_tuple,
            specific: specific_tuple,
        });
    }

    let generic_context = general_specialization.generic_context(db);
    if generic_context
        .variables(db)
        .zip(general_specialization.types(db))
        .zip(specific_specialization.types(db))
        .any(|((_, general), specific)| general != specific && !general.is_non_divergent_dynamic())
    {
        return None;
    }

    if generic_context
        .variables(db)
        .zip(general_specialization.types(db))
        .zip(specific_specialization.types(db))
        .any(|((typevar, general), specific)| {
            general != specific
                && matches!(
                    specialization_variance(db, typevar),
                    TypeVarVariance::Covariant | TypeVarVariance::Contravariant
                )
        })
    {
        if specific.has_dynamic(db) {
            return None;
        }
        Some(DynamicGeneralization::RebuildSpecialization {
            class: general_class,
            general: general_specialization,
            specific: specific_specialization,
        })
    } else {
        Some(DynamicGeneralization::UseSpecific(specific))
    }
}
