// XXX

use crate::Db;

pub(crate) trait Constraints<'db>: Clone + Sized {
    fn never(db: &'db dyn Db) -> Self;
    fn always(db: &'db dyn Db) -> Self;
    fn is_never(&self, db: &'db dyn Db) -> bool;
    fn is_always(&self, db: &'db dyn Db) -> bool;
    fn union(&mut self, db: &'db dyn Db, other: Self) -> bool;
    fn intersect(&mut self, db: &'db dyn Db, other: Self) -> bool;
    fn negate(self, db: &'db dyn Db) -> Self;

    fn from_bool(db: &'db dyn Db, b: bool) -> Self {
        if b { Self::always(db) } else { Self::never(db) }
    }

    fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never(db) {
            self.intersect(db, other());
        }
        self
    }

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

pub(crate) trait OptionConstraintsExtension<T> {
    fn when_none_or<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C;
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

pub(crate) trait IteratorConstraintsExtension<T> {
    fn when_any<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnMut(T) -> C) -> C;
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
