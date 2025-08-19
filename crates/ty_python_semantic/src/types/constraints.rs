//! Constraints under which type properties hold
//!
//! For "concrete" types (which contain no type variables), type properties like assignability have
//! simple answers: one type is either assignable to another type, or it isn't. (The _rules_ for
//! comparing two particular concrete types can be rather complex, but the _answer_ is a simple
//! "yes" or "no".)
//!
//! These properties are more complex when type variables are involved, because there are (usually)
//! many different concrete types that a typevar can be specialized to, and the type property might
//! hold for some specializations, but not for others. That means that for types that include
//! typevars, "Is this type assignable to another?" no longer makes sense as a question. The better
//! question is: "Under what constraints is this type assignable to another?".
//!
//! This module provides the machinery for representing the "under what constraints" part of that
//! question. An individual constraint restricts the specialization of a single typevar to be within a
//! particular lower and upper bound. You can then build up more complex constraint sets using
//! union, intersection, and negation operations (just like types themselves).
//!
//! NOTE: This module is currently in a transitional state: we've added a trait that our constraint
//! set implementations will conform to, and updated all of our type property implementations to
//! work on any impl of that trait. But the only impl we have right now is `bool`, which means that
//! we are still not tracking the full detail as promised in the description above. (`bool` is a
//! perfectly fine impl, but it can generate false positives when you have to break down a
//! particular assignability check into subchecks: each subcheck might say "yes", but technically
//! under conflicting constraints, which a single `bool` can't track.) Soon we will add a proper
//! constraint set implementation, and the `bool` impl of the trait (and possibly the trait itself)
//! will go away.

use smallvec::{SmallVec, smallvec};

use crate::Db;
use crate::types::{IntersectionType, Type, TypeVarInstance, UnionType};

/// Encodes the constraints under which a type property (e.g. assignability) holds.
pub(crate) trait Constraints<'db>: Clone + Sized {
    /// Returns a constraint set that never holds
    fn unsatisfiable(db: &'db dyn Db) -> Self;

    /// Returns a constraint set that always holds
    fn always_satisfiable(db: &'db dyn Db) -> Self;

    /// Returns whether this constraint set never holds
    fn is_never_satisfied(&self, db: &'db dyn Db) -> bool;

    /// Returns whether this constraint set always holds
    fn is_always_satisfied(&self, db: &'db dyn Db) -> bool;

    /// Updates this constraint set to hold the union of itself and another constraint set.
    fn union(&mut self, db: &'db dyn Db, other: Self) -> &Self;

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    fn intersect(&mut self, db: &'db dyn Db, other: Self) -> &Self;

    /// Returns the negation of this constraint set.
    fn negate(self, db: &'db dyn Db) -> Self;

    /// Returns a constraint set representing a boolean condition.
    fn from_bool(db: &'db dyn Db, b: bool) -> Self {
        if b {
            Self::always_satisfiable(db)
        } else {
            Self::unsatisfiable(db)
        }
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never_satisfied(db) {
            self.intersect(db, other());
        }
        self
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    fn or(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_always_satisfied(db) {
            self.union(db, other());
        }
        self
    }
}

impl<'db> Constraints<'db> for bool {
    fn unsatisfiable(_db: &'db dyn Db) -> Self {
        false
    }

    fn always_satisfiable(_db: &'db dyn Db) -> Self {
        true
    }

    fn is_never_satisfied(&self, _db: &'db dyn Db) -> bool {
        !*self
    }

    fn is_always_satisfied(&self, _db: &'db dyn Db) -> bool {
        *self
    }

    fn union(&mut self, _db: &'db dyn Db, other: Self) -> &Self {
        *self = *self || other;
        self
    }

    fn intersect(&mut self, _db: &'db dyn Db, other: Self) -> &Self {
        *self = *self && other;
        self
    }

    fn negate(self, _db: &'db dyn Db) -> Self {
        !self
    }
}

/// An extension trait for building constraint sets from [`Option`] values.
pub(crate) trait OptionConstraintsExtension<T> {
    /// Returns [`always_satisfiable`][Constraints::always_satisfiable] if the option is `None`;
    /// otherwise applies a function to determine under what constraints the value inside of it
    /// holds.
    fn when_none_or<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C;

    /// Returns [`unsatisfiable`][Constraints::unsatisfiable] if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_some_and<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C;
}

impl<T> OptionConstraintsExtension<T> for Option<T> {
    fn when_none_or<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C {
        match self {
            Some(value) => f(value),
            None => C::always_satisfiable(db),
        }
    }

    fn when_some_and<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnOnce(T) -> C) -> C {
        match self {
            Some(value) => f(value),
            None => C::unsatisfiable(db),
        }
    }
}

/// An extension trait for building constraint sets from an [`Iterator`].
pub(crate) trait IteratorConstraintsExtension<T> {
    /// Returns the constraints under which any element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_always_satisfied`][Constraints::is_always_satisfied] true, then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_any<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnMut(T) -> C) -> C;

    /// Returns the constraints under which every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_never_satisfied`][Constraints::is_never_satisfied] true, then the overall result must
    /// be as well, and we stop consuming elements from the iterator.
    fn when_all<'db, C: Constraints<'db>>(self, db: &'db dyn Db, f: impl FnMut(T) -> C) -> C;
}

impl<I, T> IteratorConstraintsExtension<T> for I
where
    I: Iterator<Item = T>,
{
    fn when_any<'db, C: Constraints<'db>>(self, db: &'db dyn Db, mut f: impl FnMut(T) -> C) -> C {
        let mut result = C::unsatisfiable(db);
        for child in self {
            if result.union(db, f(child)).is_always_satisfied(db) {
                return result;
            }
        }
        result
    }

    fn when_all<'db, C: Constraints<'db>>(self, db: &'db dyn Db, mut f: impl FnMut(T) -> C) -> C {
        let mut result = C::always_satisfiable(db);
        for child in self {
            if result.intersect(db, f(child)).is_never_satisfied(db) {
                return result;
            }
        }
        result
    }
}

/// A set of constraint clauses, representing the union of those clauses.
///
/// This is called a "set of constraint sets", and denoted _𝒮_, in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug)]
pub(crate) struct ConstraintSet<'db> {
    clauses: SmallVec<[ConstraintClause<'db>; 2]>,
}

impl<'db> ConstraintSet<'db> {
    fn empty() -> Self {
        Self {
            clauses: smallvec![],
        }
    }

    fn singleton(clause: ConstraintClause<'db>) -> Self {
        Self {
            clauses: smallvec![clause],
        }
    }

    /// Adds a new clause to this set, ensuring that no clause in the set subsumes another.
    fn add(&mut self, db: &'db dyn Db, clause: ConstraintClause<'db>) {
        for existing in &mut self.clauses {
            // If there is an existing constraint set that subsumes (or is subsumed by) the new
            // one, we want to keep the _subsumed_ constraint set.
            if clause.subsumes(db, existing) {
                return;
            } else if existing.subsumes(db, &clause) {
                *existing = clause;
                return;
            }
        }
        self.clauses.push(clause);
    }
}

impl<'db> Constraints<'db> for ConstraintSet<'db> {
    fn unsatisfiable(_db: &'db dyn Db) -> Self {
        Self::empty()
    }

    fn always_satisfiable(_db: &'db dyn Db) -> Self {
        Self::singleton(ConstraintClause::empty())
    }

    fn is_never_satisfied(&self, _db: &'db dyn Db) -> bool {
        self.clauses.is_empty()
    }

    fn is_always_satisfied(&self, _db: &'db dyn Db) -> bool {
        self.clauses.len() == 1 && self.clauses[0].constraints.is_empty()
    }

    fn union(&mut self, db: &'db dyn Db, other: Self) -> &Self {
        for clause in other.clauses {
            self.add(db, clause);
        }
        self
    }

    fn intersect(&mut self, db: &'db dyn Db, other: Self) -> &Self {
        let self_clauses = std::mem::take(&mut self.clauses);
        for self_clause in self_clauses {
            for other_clause in &other.clauses {
                let mut new_clause = self_clause.clone();
                new_clause.combine(db, other_clause);
                self.add(db, new_clause);
            }
        }
        self
    }

    fn negate(self, db: &'db dyn Db) -> Self {
        let mut result = Self::always_satisfiable(db);
        for clause in &self.clauses {
            result.intersect(db, clause.negate(db));
        }
        result
    }
}

/// A set of merged constraints, representing the intersection of those constraints. We guarantee
/// that no constraint in the set subsumes another, and that no two constraints in the set have the
/// same typevar.
///
/// This is called a "constraint set", and denoted _C_, in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug)]
pub(crate) struct ConstraintClause<'db> {
    constraints: SmallVec<[AtomicConstraint<'db>; 1]>,
}

impl<'db> ConstraintClause<'db> {
    fn empty() -> Self {
        Self {
            constraints: smallvec![],
        }
    }

    fn singleton(constraint: AtomicConstraint<'db>) -> Self {
        Self {
            constraints: smallvec![constraint],
        }
    }

    /// Adds a new constraint to this clause, ensuring that no constraint in the clause subsumes
    /// another, and that no two constraints in the set have the same typevar.
    fn add(&mut self, db: &'db dyn Db, constraint: AtomicConstraint<'db>) {
        for existing in &mut self.constraints {
            if constraint.typevar == existing.typevar {
                existing.merge(db, constraint);
                return;
            }
        }
        self.constraints.push(constraint);
    }

    /// Combines two constraint clauses, merging any constraints that share the same typevar.
    fn combine(&mut self, db: &'db dyn Db, other: &Self) {
        for constraint in &other.constraints {
            self.add(db, *constraint);
        }
    }

    /// Returns whether this constraint set subsumes `other` — if every constraint in `other` is
    /// subsumed by some constraint in `self`.
    fn subsumes(&self, db: &'db dyn Db, other: &Self) -> bool {
        other.constraints.iter().all(|other_constraint| {
            self.constraints
                .iter()
                .any(|self_constraint| self_constraint.subsumes(db, *other_constraint))
        })
    }

    fn negate(&self, db: &'db dyn Db) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::empty();
        for constraint in &self.constraints {
            constraint.negate_into(db, &mut result);
        }
        result
    }
}

/// A constraint that the type `s` must be a subtype of the type `t`. Tallying will find all
/// substitutions of any type variables in `s` and `t` that ensure that this subtyping holds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AtomicConstraint<'db> {
    pub(crate) lower: Type<'db>,
    pub(crate) typevar: TypeVarInstance<'db>,
    pub(crate) upper: Type<'db>,
}

impl<'db> AtomicConstraint<'db> {
    /// Returns whether this constraint subsumes `other` — if it constrains the same typevar and
    /// has tighter bounds.
    fn subsumes(self, db: &'db dyn Db, other: Self) -> bool {
        self.typevar == other.typevar
            && other.lower.is_assignable_to(db, self.lower)
            && self.upper.is_assignable_to(db, other.upper)
    }

    /// Merges another constraint into this one. Panics if the two constraints have different
    /// typevars.
    fn merge(&mut self, db: &'db dyn Db, other: Self) {
        debug_assert!(self.typevar == other.typevar);
        self.lower = UnionType::from_elements(db, [self.lower, other.lower]);
        self.upper = IntersectionType::from_elements(db, [self.upper, other.upper]);
    }

    fn negate_into(self, db: &'db dyn Db, set: &mut ConstraintSet<'db>) {
        if !self.lower.is_never() {
            let negated_lower = AtomicConstraint {
                lower: Type::Never,
                typevar: self.typevar,
                // TODO: <: not ≤:
                upper: self.lower,
            };
            set.add(db, ConstraintClause::singleton(negated_lower));
        }
        if !self.upper.is_object(db) {
            let negated_upper = AtomicConstraint {
                // TODO: <: not ≤:
                lower: self.upper,
                typevar: self.typevar,
                upper: Type::object(db),
            };
            set.add(db, ConstraintClause::singleton(negated_upper));
        }
    }
}
