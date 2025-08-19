//! Constraints under which type properties hold

use crate::Db;

/// Encodes the constraints under which a type property (e.g. assignability) holds.
pub(crate) trait Constraints<'db>: Clone + Sized {
    /// Returns a constraint set that never holds
    fn never(db: &'db dyn Db) -> Self;

    /// Returns a constraint set that always holds
    fn always(db: &'db dyn Db) -> Self;

    /// Returns whether this constraint set never holds
    fn is_never(&self, db: &'db dyn Db) -> bool;

    /// Returns whether this constraint set always holds
    fn is_always(&self, db: &'db dyn Db) -> bool;

    /// Updates this constraint set to hold the union of itself and another constraint set. Returns
    /// whether the result [`is_always`][Self::is_always]. (We use this to implement
    /// short-circuiting; once a constraint set is always true, unioning anything else into it is
    /// by definition a no-op.)
    fn union(&mut self, db: &'db dyn Db, other: Self) -> bool;

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    /// Returns whether the result [`is_never`][Self::is_always]. (We use this to implement
    /// short-circuiting; once a constraint set is always false, intersecting anything else into it
    /// is by definition a no-op.)
    fn intersect(&mut self, db: &'db dyn Db, other: Self) -> bool;

    /// Returns the negation of this constraint set.
    fn negate(self, db: &'db dyn Db) -> Self;

    /// Returns a constraint set representing a boolean condition.
    fn from_bool(db: &'db dyn Db, b: bool) -> Self {
        if b { Self::always(db) } else { Self::never(db) }
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never(db) {
            self.intersect(db, other());
        }
        self
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    fn or(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_always(db) {
            self.union(db, other());
        }
        self
    }
}

impl<'db> Constraints<'db> for bool {
    fn never(_db: &'db dyn Db) -> Self {
        false
    }

    fn always(_db: &'db dyn Db) -> Self {
        true
    }

    fn is_never(&self, _db: &'db dyn Db) -> bool {
        !*self
    }

    fn is_always(&self, _db: &'db dyn Db) -> bool {
        *self
    }

    fn union(&mut self, db: &'db dyn Db, other: Self) -> bool {
        *self = *self || other;
        self.is_always(db)
    }

    fn intersect(&mut self, db: &'db dyn Db, other: Self) -> bool {
        *self = *self && other;
        self.is_never(db)
    }

    fn negate(self, _db: &'db dyn Db) -> Self {
        !self
    }
}

/// An extension trait for building constraint sets from [`Option`] values.
pub(crate) trait OptionConstraintsExtension<T> {
    /// Returns [`always`][Constraints::always] if the option is `None`; otherwise applies a
    /// function to determine under what constraints the value inside of it holds.
    fn when_none_or<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C;

    /// Returns [`never`][Constraints::never] if the option is `None`; otherwise applies a
    /// function to determine under what constraints the value inside of it holds.
    fn when_some_and<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C;
}

impl<T> OptionConstraintsExtension<T> for Option<T> {
    fn when_none_or<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C {
        match self {
            Some(value) => f(value),
            None => C::always(db),
        }
    }

    fn when_some_and<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C {
        match self {
            Some(value) => f(value),
            None => C::never(db),
        }
    }
}

/// An extension trait for building constraint sets from an [`Iterator`].
pub(crate) trait IteratorConstraintsExtension<T> {
    /// Returns the constraints under when any element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_always`][Constraints::is_always] true, then the overall result must be as well, and we
    /// stop consuming elements from the iterator.
    fn when_any<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnMut(T) -> C) -> C;

    /// Returns the constraints under when every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_never`][Constraints::is_never] true, then the overall result must be as well, and we
    /// stop consuming elements from the iterator.
    fn when_all<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnMut(T) -> C) -> C;
}

impl<I, T> IteratorConstraintsExtension<T> for I
where
    I: Iterator<Item = T>,
{
    fn when_any<'db, C: Constraints<'db>>(self, db: &'db dyn Db, mut f: impl FnMut(T) -> C) -> C {
        let mut result = C::never(db);
        for child in self {
            if result.union(db, f(child)) {
                return result;
            }
        }
        result
    }

    fn when_all<'db, C: Constraints<'db>>(self, db: &'db dyn Db, mut f: impl FnMut(T) -> C) -> C {
        let mut result = C::always(db);
        for child in self {
            if result.intersect(db, f(child)) {
                return result;
            }
        }
        result
    }
}
