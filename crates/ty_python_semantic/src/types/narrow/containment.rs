use crate::SemanticContext;
use crate::types::{ClassBase, IntersectionBuilder, KnownClass, Type, UnionBuilder};

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
fn containment_behavior<'db>(
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
) -> ContainmentBehavior<'db> {
    let db = ctx.db();
    let ty = ty.resolve_type_alias(ctx);

    match ty {
        Type::Union(union) => {
            // Combine element containment for `not in`, ordinary `in` unions, and unions reached
            // through wrappers such as type variables. Positive unions that contain string
            // literals are distributed in `evaluate_expr_in` instead because substring semantics
            // depend on the value of each literal haystack.
            let mut builder = UnionBuilder::new(ctx);
            let mut has_unknown_behavior = false;
            for element in union.elements(db) {
                match containment_behavior(ctx, *element) {
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
            .bound_or_constraints(ctx)
            .map_or(ContainmentBehavior::Unknown, |bound_or_constraints| {
                containment_behavior(ctx, bound_or_constraints.as_type(ctx))
            }),
        Type::NewTypeInstance(newtype) => {
            containment_behavior(ctx, newtype.concrete_base_type(ctx))
        }
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
                intersection.map_positive(ctx, |element| {
                    match containment_behavior(ctx, *element) {
                        ContainmentBehavior::ElementsOf(elements_of) => {
                            has_elements_of = true;
                            elements_of
                        }
                        ContainmentBehavior::Custom => {
                            has_custom_behavior = true;
                            *element
                        }
                        ContainmentBehavior::Unknown => *element,
                    }
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
            for base in instance.class(ctx).iter_mro(ctx) {
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
                    return ContainmentBehavior::ElementsOf(Type::instance(ctx, class));
                }
                if !class
                    .own_class_member(ctx, None, "__contains__")
                    .is_undefined()
                {
                    return ContainmentBehavior::Custom;
                }
            }
            if instance.class(ctx).is_final(db) {
                ContainmentBehavior::ElementsOf(ty)
            } else {
                ContainmentBehavior::Unknown
            }
        }
        _ => ContainmentBehavior::Unknown,
    }
}

/// Return the type whose iterated elements may satisfy membership for `ty`.
pub(super) fn elements_of<'db>(ctx: &SemanticContext<'db>, ty: Type<'db>) -> Option<Type<'db>> {
    match containment_behavior(ctx, ty) {
        ContainmentBehavior::ElementsOf(elements_of) => Some(elements_of),
        ContainmentBehavior::Custom | ContainmentBehavior::Unknown => None,
    }
}

/// Maximum haystack length for which we synthesize a negative type per character.
const MAX_STRING_MEMBERSHIP_EXCLUSIONS: usize = 128;

/// Narrow membership in a known string literal using substring semantics.
pub(super) fn narrow_string_membership<'db>(
    ctx: &SemanticContext<'db>,
    lhs_ty: Type<'db>,
    haystack: &str,
    is_contained: bool,
) -> Option<Type<'db>> {
    let db = ctx.db();
    let lhs_ty = lhs_ty.resolve_type_alias(ctx);
    let flattened_lhs_ty = lhs_ty.flatten_typevars(ctx);
    let keep = |element: &Type<'db>| {
        let element = element.resolve_type_alias(ctx);
        if let Some(needle) = element.as_string_literal() {
            haystack.contains(needle.value(db)) == is_contained
        } else {
            !(is_contained && element.is_disjoint_from(ctx, KnownClass::Str.to_instance(ctx)))
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
        let mut builder = IntersectionBuilder::new(ctx).add_positive(narrowed);
        for character in haystack.chars() {
            builder = builder.add_negative(Type::single_char_string_literal(db, character));
        }
        narrowed = builder.build();
    }

    (narrowed != flattened_lhs_ty).then_some(narrowed)
}
