use crate::types::Type;
use crate::{Db, ProgramEnvironment};

mod shared;

pub(in crate::types) use shared::{
    GenericIntersectionEffects, GenericIntersectionFacts, base_top_intersection_with,
    dynamic_generalization_intersection_with, generic_gradual_intersection_with,
};

pub(in crate::types) enum GenericIntersection<'db> {
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

/// Simplify same-class gradual intersections and intersections with top-materialized subclasses.
///
/// For example, `list[int] & list[Any]` simplifies to `list[int]`, while
/// `Sequence[int] & Sequence[Any]` simplifies to `Sequence[int & Any]`.
/// A recursive pair is returned separately so callers can also skip relation-based reductions.
pub(in crate::types) fn generic_gradual_intersection<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
) -> Option<GenericIntersection<'db>> {
    match shared::generic_gradual_intersection_sync(
        left,
        right,
        &shared::OrdinaryGenericIntersectionEffects::new(db, env),
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}
