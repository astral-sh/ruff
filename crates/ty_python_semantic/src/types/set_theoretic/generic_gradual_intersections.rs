use crate::types::generics::specialization_variance;
use crate::types::tuple::TupleSpec;
use crate::types::visitor::contains_growing_type;
use crate::types::{
    ClassBase, ClassType, IntersectionType, KnownClass, MaterializationKind, Type, TypeVarVariance,
    UnionType,
};
use crate::{Db, ProgramEnvironment};

pub(super) enum GenericIntersection<'db> {
    /// Replace both intersection elements with this equivalent type.
    /// For example, intersecting `list[int]` and `list[Any]` produces `list[int]`.
    Simplified(Type<'db>),

    /// Preserve both elements and skip subsequent subtype and disjointness checks for this pair.
    ///
    /// Those checks can expand a recursive generic type's body and rebuild the intersection with
    /// different type arguments. For example:
    ///
    /// ```python
    /// class Co[T]:
    ///     def get(self) -> T: ...
    ///
    /// class Child[T](Co[T]): ...
    ///
    /// type Growing[T] = T | list[Co[Growing[list[T]]] & Child[object]]
    /// ```
    ///
    /// Checking `Co[Growing[int]] & Child[object]` can require checking
    /// `Co[Growing[list[int]]] & Child[object]`, then another intersection with `list[list[int]]`,
    /// and so on. Unlike returning `None`, this result tells the caller not to try other reductions
    /// that could restart that expansion.
    Recursive,
}

/// Simplify the intersection of two generic specializations when one is a gradual
/// generalization of the other.
///
/// For example, `list[int] & list[Any]` simplifies to `list[int]`, while
/// `Sequence[int] & Sequence[Any]` simplifies to `Sequence[int & Any]`.
/// A recursive pair is returned separately so callers can also skip relation-based reductions.
pub(super) fn generic_gradual_intersection<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
) -> Option<GenericIntersection<'db>> {
    dynamic_generalization_intersection(db, env, left, right)
        .or_else(|| dynamic_generalization_intersection(db, env, right, left))
        .map(GenericIntersection::Simplified)
        .or_else(|| nominal_top_intersection(db, env, left, right))
        .or_else(|| nominal_top_intersection(db, env, right, left))
}

/// Intersect a fully static nominal base with a generic subclass.
///
/// The subclass's identity MRO determines which subclass type variables specialize the base.
/// Restricting those variables by the base's variance preserves invariant subclass
/// materializations instead of incorrectly collapsing, for example,
/// `Sequence[int] & Top[list[Any]]` to `list[int]`.
/// Subclass arguments that do not specialize the base retain their gradualness.
fn nominal_top_intersection<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    base: Type<'db>,
    subclass: Type<'db>,
) -> Option<GenericIntersection<'db>> {
    if !base.is_nominal_instance() || !subclass.is_nominal_instance() || base.has_dynamic(db, env) {
        return None;
    }

    let (base_class, base_specialization) = base.class_specialization(db, env)?;
    let (subclass_class, subclass_specialization) = subclass.class_specialization(db, env)?;

    if base_class == subclass_class {
        return None;
    }

    let inherited_specialization = subclass_class
        .identity_specialization(db)
        .iter_mro(db)
        .find_map(|ancestor| match ancestor {
            ClassBase::Class(ClassType::Generic(alias)) if alias.origin(db) == base_class => {
                Some(alias.specialization(db))
            }
            _ => None,
        })?;

    // Inspect lazy attributes only after establishing that the classes are related. Expanding a
    // recursive generic alias or member can re-enter intersection simplification with ever-growing
    // type arguments. Exact recursive specializations can still be checked.
    if contains_growing_type(db, env, base) {
        return Some(GenericIntersection::Recursive);
    }
    if base_specialization.materialization_kind(db).is_some()
        || subclass_specialization.materialization_kind(db) == Some(MaterializationKind::Bottom)
        || !base.is_fully_static(db, env)
    {
        return None;
    }

    let subclass_context = subclass_specialization.generic_context(db);
    let mut types = subclass_specialization.types(db).to_vec();
    let mut changed = false;

    for ((base_typevar, base_type), inherited_type) in base_specialization
        .generic_context(db)
        .variables(db)
        .zip(base_specialization.types(db))
        .zip(inherited_specialization.types(db))
    {
        let Type::TypeVar(subclass_typevar) = *inherited_type else {
            return None;
        };
        let subclass_index = subclass_context
            .variables(db)
            .position(|typevar| typevar.identity(db) == subclass_typevar.identity(db))?;
        let subclass_type = types[subclass_index];

        if subclass_type == *base_type {
            continue;
        }

        let is_top_generalization = match specialization_variance(db, subclass_typevar) {
            TypeVarVariance::Covariant => subclass_type == Type::object(),
            TypeVarVariance::Contravariant => subclass_type.is_never(),
            TypeVarVariance::Invariant => {
                subclass_specialization.materialization_kind(db) == Some(MaterializationKind::Top)
                    && subclass_type.is_non_divergent_dynamic()
            }
            TypeVarVariance::Bivariant => false,
        };

        if !is_top_generalization {
            return None;
        }

        types[subclass_index] = match specialization_variance(db, base_typevar) {
            TypeVarVariance::Covariant => {
                IntersectionType::from_two_elements(db, env, subclass_type, *base_type)
            }
            TypeVarVariance::Contravariant => {
                UnionType::from_two_elements(db, env, subclass_type, *base_type)
            }
            TypeVarVariance::Invariant => *base_type,
            TypeVarVariance::Bivariant => return None,
        };
        changed = true;
    }

    if !changed {
        return None;
    }

    if subclass_class.known(db) == Some(KnownClass::Tuple) {
        let tuple = subclass_specialization.tuple(db)?;
        // A homogeneous tuple would lose the shape of any fixed prefix or suffix.
        if tuple.fixed_elements().next().is_some() {
            return None;
        }
        let TupleSpec::Variable(variable) = tuple else {
            return None;
        };
        variable.variable().homogeneous_type()?;
        return Some(GenericIntersection::Simplified(Type::homogeneous_tuple(
            db, env, types[0],
        )));
    }

    let specialization = subclass_context.specialize(db, types);
    let specialized = Type::instance(
        db,
        env,
        subclass_class.apply_optional_specialization(db, Some(specialization)),
    );
    Some(GenericIntersection::Simplified(
        if subclass_specialization.materialization_kind(db) == Some(MaterializationKind::Top) {
            specialized.top_materialization(db, env)
        } else {
            specialized
        },
    ))
}

/// Intersect two specializations of the same generic class if `general` only differs from
/// `specific` by using dynamic types.
///
/// For example, `list[Any]` dynamically generalizes `list[int]`, while `list[str]` does not.
fn dynamic_generalization_intersection<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    general: Type<'db>,
    specific: Type<'db>,
) -> Option<Type<'db>> {
    // Fast path to avoid performance regressions.
    if !general.has_dynamic(db, env)
        || matches!(general, Type::TypeVar(_) | Type::NewTypeInstance(_))
        || matches!(specific, Type::TypeVar(_) | Type::NewTypeInstance(_))
    {
        return None;
    }

    let (
        Some((general_class, general_specialization)),
        Some((specific_class, specific_specialization)),
    ) = (
        general.class_specialization(db, env),
        specific.class_specialization(db, env),
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
        let general_tuple = general_specialization.tuple(db)?;
        let specific_tuple = specific_specialization.tuple(db)?;

        if let (TupleSpec::Variable(general_variable), TupleSpec::Variable(specific_variable)) =
            (general_tuple, specific_tuple)
        {
            if general_tuple.fixed_elements().next().is_some()
                || specific_tuple.fixed_elements().next().is_some()
                || specific.has_dynamic(db, env)
            {
                return None;
            }

            let general_element = general_variable.variable().homogeneous_type()?;
            if !general_element.is_non_divergent_dynamic() {
                return None;
            }
            let specific_element = specific_variable.variable().homogeneous_type()?;

            return Some(Type::homogeneous_tuple(
                db,
                env,
                IntersectionType::from_two_elements(db, env, specific_element, general_element),
            ));
        }

        let general_tuple = general_tuple.as_fixed_length()?;
        let specific_tuple = specific_tuple.as_fixed_length()?;
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
        if specific.has_dynamic(db, env) {
            return None;
        }

        return Some(Type::heterogeneous_tuple(
            db,
            env,
            specific_tuple
                .iter_all_elements()
                .zip(general_tuple.iter_all_elements())
                .map(|(specific, general)| {
                    IntersectionType::from_two_elements(db, env, specific, general)
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

    if specific.has_dynamic(db, env) {
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
                    IntersectionType::from_two_elements(db, env, *specific, *general)
                }
                TypeVarVariance::Contravariant => {
                    UnionType::from_two_elements(db, env, *specific, *general)
                }
                TypeVarVariance::Invariant | TypeVarVariance::Bivariant => *specific,
            }
        })
        .collect();
    let specialization = generic_context.specialize(db, types);

    Some(Type::instance(
        db,
        env,
        general_class.apply_optional_specialization(db, Some(specialization)),
    ))
}
