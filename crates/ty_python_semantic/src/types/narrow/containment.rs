use crate::{
    Db,
    types::{ClassBase, IntersectionBuilder, KnownClass, Type, UnionBuilder},
};

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
///
/// For subclasses that inherit a known built-in `__contains__`, the stored type is the specialized
/// built-in base rather than `ty`. This preserves the built-in element type even if the subclass
/// overrides `__iter__`.
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
        Type::TypedDict(_) => ContainmentBehavior::ElementsOf(ty),
        Type::NominalInstance(instance) => {
            // Walk the MRO until we find either a visible override or a supported built-in
            // implementation. Returning the built-in base preserves its specialization; normal
            // member lookup specializes inherited signatures for the subclass instead.
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
