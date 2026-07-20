use crate::{
    Db,
    types::{
        ClassBase, IntersectionBuilder, KnownClass, Type, UnionBuilder,
        equality::{equality_exclusion_constraint, equality_truthiness},
    },
};
use ruff_python_ast as ast;
use ty_python_core::Truthiness;

enum ContainmentBehavior<'db> {
    /// Membership compares against the elements yielded by the wrapped type. Callers use
    /// [`Type::try_iterate`] to determine the types that may be contained.
    ElementsOf(Type<'db>),
    /// A statically visible `__contains__` defines different membership behavior.
    Custom,
    /// No containment behavior can be established from the static type.
    Unknown,
}

/// Return the containment behavior known for this type.
fn containment_behavior<'db>(db: &'db dyn Db, ty: Type<'db>) -> ContainmentBehavior<'db> {
    let ty = ty.resolve_type_alias(db);

    match ty {
        Type::Union(union) => {
            // Combine element containment for `not in`, ordinary `in` unions, and unions reached
            // through wrappers such as type variables. Positive unions that contain string
            // literals are distributed in `evaluate_expr_in` instead because substring semantics
            // depend on the value of each literal haystack.
            let mut builder = UnionBuilder::new(db);
            let mut has_unknown_behavior = false;
            for element in union.elements(db) {
                match containment_behavior(db, *element) {
                    ContainmentBehavior::ElementsOf(elements_of) => {
                        builder = builder.add(elements_of);
                    }
                    ContainmentBehavior::Custom => return ContainmentBehavior::Custom,
                    ContainmentBehavior::Unknown => has_unknown_behavior = true,
                }
            }
            if has_unknown_behavior {
                ContainmentBehavior::Unknown
            } else {
                ContainmentBehavior::ElementsOf(builder.build())
            }
        }
        Type::TypeVar(type_var) => type_var
            .typevar(db)
            .bound_or_constraints(db)
            .map_or(ContainmentBehavior::Unknown, |bound_or_constraints| {
                containment_behavior(db, bound_or_constraints.as_type(db))
            }),
        Type::NewTypeInstance(newtype) => containment_behavior(db, newtype.concrete_base_type(db)),
        Type::Intersection(intersection) => {
            // Preserve the narrowing already supported on main for unsimplified intersections
            // such as `Iterable[T] & tuple[object, ...]`. Replacing the component that establishes
            // containment with its stored iteration type while retaining the unknown component
            // lets `try_iterate` recover `T`. This is a temporary workaround for the intersection
            // not simplifying to `tuple[T, ...]`; remove this arm once that simplification is
            // implemented:
            // https://github.com/astral-sh/ty/issues/3890
            // The generic intersection work in the following PR is intended to support that fix:
            // https://github.com/astral-sh/ruff/pull/26365
            let mut has_elements_of = false;
            let mut has_custom_behavior = false;
            let elements_of =
                intersection.map_positive(db, |element| match containment_behavior(db, *element) {
                    ContainmentBehavior::ElementsOf(elements_of) => {
                        has_elements_of = true;
                        elements_of
                    }
                    ContainmentBehavior::Custom => {
                        has_custom_behavior = true;
                        *element
                    }
                    ContainmentBehavior::Unknown => *element,
                });
            if has_custom_behavior {
                ContainmentBehavior::Custom
            } else if has_elements_of {
                ContainmentBehavior::ElementsOf(elements_of)
            } else {
                ContainmentBehavior::Unknown
            }
        }
        Type::TypedDict(_) => ContainmentBehavior::ElementsOf(ty),
        Type::NominalInstance(instance) => {
            // Walk the MRO until we find either a visible override or a supported built-in
            // implementation.
            for base in instance.class(db).iter_mro(db) {
                let class = match base {
                    ClassBase::Class(class) => class,
                    ClassBase::Generic | ClassBase::Protocol => continue,
                    ClassBase::Any
                    | ClassBase::Dynamic(_)
                    | ClassBase::Divergent(_)
                    | ClassBase::TypedDict(_) => return ContainmentBehavior::Unknown,
                };
                if matches!(
                    class.known(db),
                    Some(
                        KnownClass::List
                            | KnownClass::Set
                            | KnownClass::FrozenSet
                            | KnownClass::Dict
                            | KnownClass::Tuple
                            | KnownClass::Range
                    )
                ) {
                    // For built-ins with a known containment behavior, we always store the
                    // built-in base (not `self`), even if `self` is a subclass of the built-in. At
                    // runtime, a custom `__iter__` on a subclass of a built-in container does not
                    // change the `__contains__` behavior of the built-in; storing the built-in
                    // instead of the subclass ensures we model that correctly. We end up calling
                    // `try_iterate` on the stored type, so otherwise we would wrongly prefer the
                    // `__iter__` annotation. (It is always true for all types that `__contains__`
                    // takes precedence over `__iter__` for containment checks, but this is only
                    // relevant to us for built-ins, since user types with `__contains__` have
                    // containment behavior that we can't understand and don't try to model.)
                    return ContainmentBehavior::ElementsOf(Type::instance(db, class));
                }
                if !class
                    .own_class_member(db, None, "__contains__")
                    .is_undefined()
                {
                    return ContainmentBehavior::Custom;
                }
            }
            if instance.class(db).is_final(db) {
                ContainmentBehavior::ElementsOf(ty)
            } else {
                ContainmentBehavior::Unknown
            }
        }
        _ => ContainmentBehavior::Unknown,
    }
}

/// Return the type whose iterated elements may satisfy membership for `ty`.
pub(super) fn elements_of<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Type<'db>> {
    match containment_behavior(db, ty) {
        ContainmentBehavior::ElementsOf(elements_of) => Some(elements_of),
        ContainmentBehavior::Custom | ContainmentBehavior::Unknown => None,
    }
}

/// Preserve the precise element types of an immediately consumed list or set literal.
///
/// These expressions cannot be mutated before membership is evaluated. Representing them as
/// fixed-length tuples lets comparison inference and negative narrowing use the same elements.
pub(super) fn inline_membership_rhs_type<'db>(
    db: &'db dyn Db,
    rhs: &ast::Expr,
    mut expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> Option<Type<'db>> {
    let elements = match rhs.expression_value() {
        ast::Expr::List(list) => &list.elts,
        ast::Expr::Set(set) => &set.elts,
        _ => return None,
    };

    if elements.iter().any(ast::Expr::is_starred_expr) {
        return None;
    }

    Some(Type::heterogeneous_tuple(
        db,
        elements.iter().map(&mut expression_type),
    ))
}

/// Return a constraint excluding every value known to compare equal to a fixed container element.
///
/// `not in` negates equality with every element; it does not use `__ne__`. Only add an exclusion
/// when every value represented by a slot is known to compare equal.
pub(super) fn membership_exclusion_constraint<'db>(
    db: &'db dyn Db,
    elements: &[Type<'db>],
) -> Option<Type<'db>> {
    let mut builder = IntersectionBuilder::new(db);
    let mut constrained = false;

    for element_ty in elements.iter().copied() {
        if let Some(constraint) = equality_exclusion_constraint(db, element_ty) {
            builder = builder.add_positive(constraint);
            constrained = true;
        }
    }

    constrained.then(|| builder.build())
}

/// Return the truthiness of an element-based membership check when its result is known.
///
/// A check is always false if no element can compare equal, and always true if applying the same
/// exclusions used for negative narrowing leaves no possible value.
pub(super) fn membership_truthiness<'db>(
    db: &'db dyn Db,
    lhs_ty: Type<'db>,
    rhs_ty: Type<'db>,
) -> Truthiness {
    let Some(iterable) = elements_of(db, rhs_ty).and_then(|ty| ty.try_iterate(db).ok()) else {
        return Truthiness::Ambiguous;
    };
    let Some(fixed_length) = iterable.as_fixed_length() else {
        return Truthiness::Ambiguous;
    };
    let elements = fixed_length.all_elements();

    if elements
        .iter()
        .all(|element| equality_truthiness(db, lhs_ty, *element).is_always_false())
    {
        return Truthiness::AlwaysFalse;
    }

    if let Some(exclusion) = membership_exclusion_constraint(db, elements)
        && IntersectionBuilder::new(db)
            .add_positive(lhs_ty)
            .add_positive(exclusion)
            .build()
            .is_never()
    {
        return Truthiness::AlwaysTrue;
    }

    Truthiness::Ambiguous
}

/// Maximum haystack length for which we synthesize a negative type per character.
const MAX_STRING_MEMBERSHIP_EXCLUSIONS: usize = 128;

/// Narrow membership in a known string literal using substring semantics.
pub(super) fn narrow_string_membership<'db>(
    db: &'db dyn Db,
    lhs_ty: Type<'db>,
    haystack: &str,
    is_contained: bool,
) -> Option<Type<'db>> {
    let lhs_ty = lhs_ty.resolve_type_alias(db);
    let flattened_lhs_ty = lhs_ty.flatten_typevars(db);
    let keep = |element: &Type<'db>| {
        let element = element.resolve_type_alias(db);
        if let Some(needle) = element.as_string_literal() {
            haystack.contains(needle.value(db)) == is_contained
        } else {
            !(is_contained && element.is_disjoint_from(db, KnownClass::Str.to_instance(db)))
        }
    };

    let mut narrowed = match flattened_lhs_ty {
        Type::Union(union) => union.filter(db, keep),
        _ if keep(&flattened_lhs_ty) => flattened_lhs_ty,
        _ => Type::Never,
    };

    if !is_contained
        && haystack
            .chars()
            .nth(MAX_STRING_MEMBERSHIP_EXCLUSIONS)
            .is_none()
    {
        let mut builder = IntersectionBuilder::new(db).add_positive(narrowed);
        for character in haystack.chars() {
            builder = builder.add_negative(Type::single_char_string_literal(db, character));
        }
        narrowed = builder.build();
    }

    (narrowed != flattened_lhs_ty).then_some(narrowed)
}
