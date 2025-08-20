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
use crate::types::{BoundTypeVarInstance, IntersectionType, Type, UnionType};

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
///
/// ### Invariants
///
/// - No clause in the set subsumes another. (That is, there is no clause in the set that is a
///   "subclause" of another.)
#[derive(Clone, Debug)]
pub(crate) struct ConstraintSet<'db> {
    clauses: SmallVec<[ConstraintClause<'db>; 2]>,
}

impl<'db> ConstraintSet<'db> {
    fn never() -> Self {
        Self {
            clauses: smallvec![],
        }
    }

    fn singleton(clause: ConstraintClause<'db>) -> Self {
        Self {
            clauses: smallvec![clause],
        }
    }

    /// Updates this set to be the union of itself and a clause.
    fn union_clause(&mut self, db: &'db dyn Db, clause: ConstraintClause<'db>) {
        // If there is an existing clause that is subsumed by (i.e., is bigger than) the new one,
        // then the new clause doesn't provide any useful additional information, and we don't have
        // to add it.
        if (self.clauses.iter()).any(|existing| clause.subsumes(db, existing)) {
            return;
        }

        // If there are any existing clauses that subsume (i.e., are smaller than) the new one, we
        // should delete them, since the new clause provides strictly more useful information.
        self.clauses
            .retain(|existing| !existing.subsumes(db, &clause));
        self.clauses.push(clause);
    }

    /// Updates this set to be the union of itself and another set.
    fn union_set(&mut self, db: &'db dyn Db, other: Self) {
        for clause in other.clauses {
            self.union_clause(db, clause);
        }
    }

    /// Updates this set to be the intersection of itself and another set.
    fn intersect_set(&mut self, db: &'db dyn Db, other: &Self) {
        let self_clauses = std::mem::take(&mut self.clauses);
        for self_clause in &self_clauses {
            for other_clause in &other.clauses {
                self.union_set(db, self_clause.intersect_clause(db, other_clause));
            }
        }
    }
}

impl<'db> Constraints<'db> for ConstraintSet<'db> {
    fn unsatisfiable(_db: &'db dyn Db) -> Self {
        Self::never()
    }

    fn always_satisfiable(_db: &'db dyn Db) -> Self {
        Self::singleton(ConstraintClause::always())
    }

    fn is_never_satisfied(&self, _db: &'db dyn Db) -> bool {
        self.clauses.is_empty()
    }

    fn is_always_satisfied(&self, _db: &'db dyn Db) -> bool {
        self.clauses.len() == 1 && self.clauses[0].constraints.is_empty()
    }

    fn union(&mut self, db: &'db dyn Db, other: Self) -> &Self {
        self.union_set(db, other);
        self
    }

    fn intersect(&mut self, db: &'db dyn Db, other: Self) -> &Self {
        self.intersect_set(db, &other);
        self
    }

    fn negate(self, db: &'db dyn Db) -> Self {
        let mut result = Self::always_satisfiable(db);
        for clause in self.clauses {
            result.intersect_set(db, &clause.negate(db));
        }
        result
    }
}

/// The intersection of a list of atomic constraints.
///
/// This is called a "constraint set", and denoted _C_, in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
///
/// ### Invariants
///
/// - No two constraints in the clause will constrain the same typevar.
#[derive(Clone, Debug)]
pub(crate) struct ConstraintClause<'db> {
    constraints: SmallVec<[AtomicConstraint<'db>; 1]>,
}

impl<'db> ConstraintClause<'db> {
    fn always() -> Self {
        Self {
            constraints: smallvec![],
        }
    }

    fn singleton(constraint: AtomicConstraint<'db>) -> Self {
        Self {
            constraints: smallvec![constraint],
        }
    }

    /// Returns the intersection of this clause and an atomic constraint. Because atomic
    /// constraints can be negated, the intersection of the new and existing atomic constraints
    /// (for the same typevar) might be the union of two atomic constraints.
    fn intersect_constraint(
        mut self,
        db: &'db dyn Db,
        constraint: AtomicConstraint<'db>,
    ) -> IntersectionResult<Self> {
        let Some((index, existing)) = (self.constraints.iter().enumerate())
            .find(|(_, existing)| existing.typevar == constraint.typevar)
        else {
            self.constraints.push(constraint);
            return IntersectionResult::One(self);
        };

        match existing.intersect(db, constraint) {
            // If the intersected constraint cannot be satisfied, that causes this whole clause to
            // be unsatisfiable too. (X ∩ 0 == 0)
            IntersectionResult::Never => IntersectionResult::Never,

            // If the intersected result is always satisfied, then the constraint no longer
            // contributes anything to the clause, and can be removed. (X ∩ 1 == X)
            IntersectionResult::Always => {
                self.constraints.swap_remove(index);
                if self.constraints.is_empty() {
                    // If there are no further constraints in the clause, the clause is now always
                    // satisfied.
                    IntersectionResult::Always
                } else {
                    IntersectionResult::One(self)
                }
            }

            // If the intersection is a single constraint, we can reuse the existing constraint's
            // place in the clause's constraint list.
            IntersectionResult::One(constraint) => {
                self.constraints[index] = constraint;
                IntersectionResult::One(self)
            }

            // If the intersection is a union of two constraints, we can reuse the existing
            // constraint's place in the clause's constraint list for one of the union elements;
            // and we must create a new clause to hold the second union eleme
            IntersectionResult::Two(first, second) => {
                let mut extra = self.clone();
                self.constraints[index] = first;
                extra.constraints[index] = second;
                IntersectionResult::Two(self, extra)
            }
        }
    }

    /// Returns the intersection of this clause with another. The result is a full constraint set,
    /// since the intersection of each constraint in the clause might result in a union of
    /// constraints.
    fn intersect_clause(&self, db: &'db dyn Db, other: &Self) -> ConstraintSet<'db> {
        let mut prev = ConstraintSet::never();
        let mut next = ConstraintSet::singleton(self.clone());
        for constraint in &other.constraints {
            std::mem::swap(&mut prev, &mut next);
            for clause in prev.clauses.drain(..) {
                match clause.intersect_constraint(db, *constraint) {
                    IntersectionResult::Never => {}
                    IntersectionResult::Always => {
                        // If any clause becomes always satisfiable, the set as a whole does too,
                        // and we don't have to process any more of the clauses.
                        next.clauses.clear();
                        next.clauses.push(ConstraintClause::always());
                        break;
                    }
                    IntersectionResult::One(clause) => {
                        next.union_clause(db, clause);
                    }
                    IntersectionResult::Two(first, second) => {
                        next.union_clause(db, first);
                        next.union_clause(db, second);
                    }
                }
            }
        }
        next
    }

    /// Returns whether this constraint set subsumes `other` — if every constraint in `other` is
    /// subsumed by some constraint in `self`. (Or equivalently, if the intersection of `self` and
    /// `other` is `self`.)
    fn subsumes(&self, db: &'db dyn Db, other: &Self) -> bool {
        other.constraints.iter().all(|other_constraint| {
            self.constraints
                .iter()
                .any(|self_constraint| self_constraint.subsumes(db, *other_constraint))
        })
    }

    /// Returns the negation of this clause.
    fn negate(&self, db: &'db dyn Db) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::never();
        for constraint in &self.constraints {
            result.union_clause(db, ConstraintClause::singleton(constraint.negate()));
        }
        result
    }
}

/// A constraint on a single typevar.
///
/// ### Invariants
///
/// - The bounds must be ordered correctly: `lower ≤: upper`. (A constraint `upper <: lower` does
///   _not_ mean that the typevar can only specialize to `Never`; it means that there is no
///   concrete type that the typevar can specialize to.)
///
/// - The bounds must actually constrain the typevar: `lower` must not be `Never` or `upper` must
///   not be `object`. (A positive constraint `Never ≤: T ≤: object` doesn't constrain the typevar
///   at all, and so we don't need a constraint in the corresponding constraint clause. A negative
///   constraint `not(Never ≤: T ≤: object)` means that there is no concrete type the typevar can
///   specialize to.)
#[derive(Clone, Copy, Debug)]
pub(crate) struct AtomicConstraint<'db> {
    sign: ConstraintSign,
    typevar: BoundTypeVarInstance<'db>,
    lower: Type<'db>,
    upper: Type<'db>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConstraintSign {
    Positive,
    Negative,
}

impl<'db> AtomicConstraint<'db> {
    /// Returns a new positive atomic constraint, ensuring that all invariants are held.
    fn positive(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Satisfiable<Self> {
        if !lower.is_assignable_to(db, upper) {
            return Satisfiable::Never;
        }
        if lower.is_never() && upper.is_object(db) {
            return Satisfiable::Always;
        }
        Satisfiable::Constrained(Self {
            sign: ConstraintSign::Positive,
            typevar,
            lower,
            upper,
        })
    }

    /// Returns a new negative atomic constraint, ensuring that all invariants are held.
    fn negative(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Satisfiable<Self> {
        if !lower.is_assignable_to(db, upper) {
            return Satisfiable::Always;
        }
        if lower.is_never() && upper.is_object(db) {
            return Satisfiable::Never;
        }
        Satisfiable::Constrained(Self {
            sign: ConstraintSign::Negative,
            typevar,
            lower,
            upper,
        })
    }

    /// Returns the negation of this atomic constraint.
    fn negate(mut self) -> Self {
        self.sign = match self.sign {
            ConstraintSign::Positive => ConstraintSign::Negative,
            ConstraintSign::Negative => ConstraintSign::Positive,
        };
        self
    }

    /// Returns whether `self` has tighter bounds than `other` — that is, if the intersection of
    /// `self` and `other` is `self`.
    fn subsumes(self, db: &'db dyn Db, other: Self) -> bool {
        debug_assert!(self.typevar == other.typevar);
        match (self.sign, other.sign) {
            (ConstraintSign::Positive, ConstraintSign::Positive) => {
                other.lower.is_assignable_to(db, self.lower)
                    && self.upper.is_assignable_to(db, other.upper)
            }

            (ConstraintSign::Negative, ConstraintSign::Negative) => {
                self.lower.is_assignable_to(db, other.lower)
                    && other.upper.is_assignable_to(db, self.upper)
            }

            (ConstraintSign::Positive, ConstraintSign::Negative) => {
                self.upper.is_assignable_to(db, other.lower)
            }

            (ConstraintSign::Negative, ConstraintSign::Positive) => false,
        }
    }

    /// Returns the intersection of this atomic constraint and another. Because constraint bounds
    /// can be negated, the result might be unsatisfiable; always satisfiable; or the union of one
    /// or two atomic constraints.
    ///
    /// Panics if the two constraints have different typevars.
    fn intersect(self, db: &'db dyn Db, other: Self) -> IntersectionResult<AtomicConstraint<'db>> {
        debug_assert!(self.typevar == other.typevar);
        match (self.sign, other.sign) {
            (ConstraintSign::Positive, ConstraintSign::Positive) => {
                IntersectionResult::from_one(Self::positive(
                    db,
                    self.typevar,
                    UnionType::from_elements(db, [self.lower, other.lower]),
                    IntersectionType::from_elements(db, [self.upper, other.upper]),
                ))
            }

            (ConstraintSign::Negative, ConstraintSign::Negative) => IntersectionResult::from_two(
                Self::negative(
                    db,
                    self.typevar,
                    IntersectionType::from_elements(db, [self.lower, other.lower]),
                    UnionType::from_elements(db, [self.upper, other.upper]),
                ),
                Self::positive(
                    db,
                    self.typevar,
                    IntersectionType::from_elements(db, [self.upper, other.upper]),
                    UnionType::from_elements(db, [self.lower, other.lower]),
                ),
            ),

            (ConstraintSign::Positive, ConstraintSign::Negative) => IntersectionResult::from_two(
                Self::positive(
                    db,
                    self.typevar,
                    self.lower,
                    IntersectionType::from_elements(db, [other.lower, self.upper]),
                ),
                Self::positive(
                    db,
                    self.typevar,
                    UnionType::from_elements(db, [self.lower, other.upper]),
                    self.upper,
                ),
            ),

            (ConstraintSign::Negative, ConstraintSign::Positive) => IntersectionResult::from_two(
                Self::positive(
                    db,
                    self.typevar,
                    other.lower,
                    IntersectionType::from_elements(db, [self.lower, other.upper]),
                ),
                Self::positive(
                    db,
                    self.typevar,
                    UnionType::from_elements(db, [other.lower, self.upper]),
                    other.upper,
                ),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Satisfiable<T> {
    Never,
    Always,
    Constrained(T),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum IntersectionResult<T> {
    Never,
    Always,
    One(T),
    Two(T, T),
}

impl<T> IntersectionResult<T> {
    fn from_one(constraint: Satisfiable<T>) -> Self {
        match constraint {
            Satisfiable::Never => IntersectionResult::Never,
            Satisfiable::Always => IntersectionResult::Always,
            Satisfiable::Constrained(constraint) => IntersectionResult::One(constraint),
        }
    }

    fn from_two(first: Satisfiable<T>, second: Satisfiable<T>) -> Self {
        match (first, second) {
            (Satisfiable::Never, _) | (_, Satisfiable::Never) => IntersectionResult::Never,
            (Satisfiable::Always, Satisfiable::Always) => IntersectionResult::Always,
            (Satisfiable::Constrained(one), Satisfiable::Always)
            | (Satisfiable::Always, Satisfiable::Constrained(one)) => IntersectionResult::One(one),
            (Satisfiable::Constrained(first), Satisfiable::Constrained(second)) => {
                IntersectionResult::Two(first, second)
            }
        }
    }
}
