// XXX
#![expect(dead_code)]

use crate::Db;
use crate::types::{BoundTypeVarInstance, Type};

/// A constraint establishing an upper and lower bound on a type variable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct Constraint<'db> {
    pub(crate) lower: Type<'db>,
    pub(crate) typevar: BoundTypeVarInstance<'db>,
    pub(crate) upper: Type<'db>,
}

impl Constraint<'_> {
    pub(crate) fn is_satisfiable(&self) -> bool {
        !self.upper.is_never()
    }
}

pub(crate) trait Constraints<'db>: Clone + Sized {
    fn never(db: &'db dyn Db) -> Self;
    fn always(db: &'db dyn Db) -> Self;
    fn from_constraint(db: &'db dyn Db, constraint: Constraint<'db>) -> Self;
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

    fn distributed_union(db: &'db dyn Db, iter: impl IntoIterator<Item = Self>) -> Self {
        let mut result = Self::never(db);
        for child in iter {
            if result.is_always(db) {
                break;
            }
            result.union(db, child);
        }
        result
    }

    fn distributed_intersection(db: &'db dyn Db, iter: impl IntoIterator<Item = Self>) -> Self {
        let mut result = Self::always(db);
        for child in iter {
            if result.is_never(db) {
                break;
            }
            result.intersect(db, child);
        }
        result
    }
}

impl<'db> Constraints<'db> for bool {
    fn never(_db: &'db dyn Db) -> Self {
        false
    }

    fn always(_db: &'db dyn Db) -> Self {
        true
    }

    fn from_constraint(_db: &'db dyn Db, constraint: Constraint<'db>) -> Self {
        constraint.is_satisfiable()
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
