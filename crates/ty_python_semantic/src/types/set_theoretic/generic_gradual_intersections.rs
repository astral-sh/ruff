use crate::Db;
use crate::types::generics::specialization_variance;
use crate::types::{IntersectionType, KnownClass, Type, TypeVarVariance, UnionType};

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
    dynamic_generalization_intersection(db, left, right)
        .or_else(|| dynamic_generalization_intersection(db, right, left))
}

/// Intersect two specializations of the same generic class if `general` only differs from
/// `specific` by using dynamic types.
///
/// For example, `list[Any]` dynamically generalizes `list[int]`, while `list[str]` does not.
fn dynamic_generalization_intersection<'db>(
    db: &'db dyn Db,
    general: Type<'db>,
    specific: Type<'db>,
) -> Option<Type<'db>> {
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

        return Some(Type::heterogeneous_tuple(
            db,
            specific_tuple
                .iter_all_elements()
                .zip(general_tuple.iter_all_elements())
                .map(|(specific, general)| {
                    IntersectionType::from_two_elements(db, specific, general)
                }),
        ));
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

    let has_variant_replacement = generic_context
        .variables(db)
        .zip(general_specialization.types(db))
        .zip(specific_specialization.types(db))
        .any(|((typevar, general), specific)| {
            general != specific
                && matches!(
                    specialization_variance(db, typevar),
                    TypeVarVariance::Covariant | TypeVarVariance::Contravariant
                )
        });

    if !has_variant_replacement {
        return Some(specific);
    }

    if specific.has_dynamic(db) {
        return None;
    }

    let types: Vec<_> = generic_context
        .variables(db)
        .zip(general_specialization.types(db))
        .zip(specific_specialization.types(db))
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

    Some(Type::instance(
        db,
        general_class.apply_optional_specialization(db, Some(specialization)),
    ))
}
