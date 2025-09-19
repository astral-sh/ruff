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
//! question.
//!
//! An individual constraint restricts the specialization of a single typevar. You can then build
//! up more complex constraint sets using union, intersection, and negation operations. We use a
//! disjunctive normal form (DNF) representation, just like we do for types: a [constraint
//! set][ConstraintSet] is the union of zero or more [clauses][ConstraintClause], each of which is
//! the intersection of zero or more [individual constraints][ConstrainedTypeVar]. Note that the
//! constraint set that contains no clauses is never satisfiable (`⋃ {} = 0`); and the constraint
//! set that contains a single clause, where that clause contains no constraints, is always
//! satisfiable (`⋃ {⋂ {}} = 1`).
//!
//! An individual constraint consists of a _positive range_ and zero or more _negative holes_. The
//! positive range and each negative hole consists of a lower and upper bound. A type is within a
//! lower and upper bound if it is a supertype of the lower bound and a subtype of the upper bound.
//! The typevar can specialize to any type that is within the positive range, and is not within any
//! of the negative holes. (You can think of the constraint as the set of types that are within the
//! positive range, with the negative holes "removed" from that set.)
//!
//! Note that all lower and upper bounds in a constraint must be fully static. We take the bottom
//! and top materializations of the types to remove any gradual forms if needed.
//!
//! NOTE: This module is currently in a transitional state. We've added the DNF [`ConstraintSet`]
//! representation, and updated all of our property checks to build up a constraint set and then
//! check whether it is ever or always satisfiable, as appropriate. We are not yet inferring
//! specializations from those constraints.
//!
//! ### Examples
//!
//! For instance, in the following Python code:
//!
//! ```py
//! class A: ...
//! class B(A): ...
//!
//! def _[T: B](t: T) -> None: ...
//! def _[U: (int, str)](u: U) -> None: ...
//! ```
//!
//! The typevar `T` has an upper bound of `B`, which would translate into the constraint `Never ≤ T
//! ≤ B`. (Every type is a supertype of `Never`, so having `Never` as a lower bound means that
//! there is effectively no lower bound. Similarly, an upper bound of `object` means that there is
//! effectively no upper bound.) The `T ≤ B` part expresses that the type can specialize to any
//! type that is a subtype of B.
//!
//! The typevar `U` is constrained to be either `int` or `str`, which would translate into the
//! constraint `(int ≤ T ≤ int) ∪ (str ≤ T ≤ str)`. When the lower and upper bounds are the same,
//! the constraint says that the typevar must specialize to that _exact_ type, not to a subtype or
//! supertype of it.

use std::fmt::Display;

use itertools::{EitherOrBoth, Itertools};
use smallvec::{SmallVec, smallvec};

use crate::Db;
use crate::types::{BoundTypeVarInstance, IntersectionType, Type, UnionType};

fn comparable<'db>(db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> bool {
    left.is_subtype_of(db, right) || right.is_subtype_of(db, left)
}

fn incomparable<'db>(db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> bool {
    !comparable(db, left, right)
}

/// An extension trait for building constraint sets from [`Option`] values.
pub(crate) trait OptionConstraintsExtension<T> {
    /// Returns a constraint set that is always satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_none_or<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db>;

    /// Returns a constraint set that is never satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_some_and<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db>;
}

impl<T> OptionConstraintsExtension<T> for Option<T> {
    fn when_none_or<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::always(),
        }
    }

    fn when_some_and<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::never(),
        }
    }
}

/// An extension trait for building constraint sets from an [`Iterator`].
pub(crate) trait IteratorConstraintsExtension<T> {
    /// Returns the constraints under which any element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_always_satisfied`][ConstraintSet::is_always_satisfied] true, then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_any<'db>(
        self,
        db: &'db dyn Db,
        f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db>;

    /// Returns the constraints under which every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_never_satisfied`][ConstraintSet::is_never_satisfied] true, then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_all<'db>(
        self,
        db: &'db dyn Db,
        f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db>;
}

impl<I, T> IteratorConstraintsExtension<T> for I
where
    I: Iterator<Item = T>,
{
    fn when_any<'db>(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::never();
        for child in self {
            if result.union(db, f(child)).is_always_satisfied() {
                return result;
            }
        }
        result
    }

    fn when_all<'db>(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::always();
        for child in self {
            if result.intersect(db, &f(child)).is_never_satisfied() {
                return result;
            }
        }
        result
    }
}

/// A set of constraints under which a type property holds.
///
/// We use a DNF representation, so a set contains a list of zero or more
/// [clauses][ConstraintClause], each of which is an intersection of zero or more
/// [constraints][ConstrainedTypeVar].
///
/// This is called a "set of constraint sets", and denoted _𝒮_, in [[POPL2015][]].
///
/// ### Invariants
///
/// - The clauses are simplified as much as possible — there are no two clauses in the set that can
///   be simplified into a single clause.
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct ConstraintSet<'db> {
    // NOTE: We use 2 here because there are a couple of places where we create unions of 2 clauses
    // as temporary values — in particular when negating a constraint — and this lets us avoid
    // spilling the temporary value to the heap.
    clauses: SmallVec<[ConstraintClause<'db>; 2]>,
}

impl<'db> ConstraintSet<'db> {
    fn never() -> Self {
        Self {
            clauses: smallvec![],
        }
    }

    fn always() -> Self {
        Self::singleton(ConstraintClause::always())
    }

    /// Returns whether this constraint set never holds
    pub(crate) fn is_never_satisfied(&self) -> bool {
        self.clauses.is_empty()
    }

    /// Returns whether this constraint set always holds
    pub(crate) fn is_always_satisfied(&self) -> bool {
        self.clauses.len() == 1 && self.clauses[0].is_always()
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    pub(crate) fn union(&mut self, db: &'db dyn Db, other: Self) -> &Self {
        self.union_set(db, other);
        self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    pub(crate) fn intersect(&mut self, db: &'db dyn Db, other: &Self) -> &Self {
        self.intersect_set(db, other);
        self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(&self, db: &'db dyn Db) -> Self {
        let mut result = Self::always();
        for clause in &self.clauses {
            result.intersect_set(db, &clause.negate(db));
        }
        result
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    pub(crate) fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never_satisfied() {
            self.intersect(db, &other());
        }
        self
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    pub(crate) fn or(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_always_satisfied() {
            self.union(db, other());
        }
        self
    }

    /// Returns a constraint set that contains a single clause.
    fn singleton(clause: ConstraintClause<'db>) -> Self {
        Self {
            clauses: smallvec![clause],
        }
    }

    pub(crate) fn range(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        let lower = lower.bottom_materialization(db);
        let upper = upper.top_materialization(db);
        let constraint = Constraint::range(db, lower, upper).constrain(typevar);
        let mut result = Self::never();
        result.union_constraint(db, constraint);
        result
    }

    pub(crate) fn negated_range(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        let lower = lower.bottom_materialization(db);
        let upper = upper.top_materialization(db);
        let constraint = Constraint::negated_range(db, lower, upper).constrain(typevar);
        let mut result = Self::never();
        result.union_constraint(db, constraint);
        result
    }

    /// Updates this set to be the union of itself and a constraint.
    fn union_constraint(
        &mut self,
        db: &'db dyn Db,
        constraint: Satisfiable<ConstrainedTypeVar<'db>>,
    ) {
        self.union_clause(db, constraint.map(ConstraintClause::singleton));
    }

    /// Updates this set to be the union of itself and a clause. To maintain the invariants of this
    /// type, we must simplify this clause against all existing clauses, if possible.
    fn union_clause(&mut self, db: &'db dyn Db, clause: Satisfiable<ConstraintClause<'db>>) {
        let mut clause = match clause {
            // If the new constraint can always be satisfied, that causes this whole set to be
            // always satisfied too.
            Satisfiable::Always => {
                self.clauses.clear();
                self.clauses.push(ConstraintClause::always());
                return;
            }

            // If the new clause can never satisfied, then the set does not change.
            Satisfiable::Never => return,

            Satisfiable::Constrained(clause) => clause,
        };

        // Naively, we would just append the new clause to the set's list of clauses. But that
        // doesn't ensure that the clauses are simplified with respect to each other. So instead,
        // we iterate through the list of existing clauses, and try to simplify the new clause
        // against each one in turn. (We can assume that the existing clauses are already
        // simplified with respect to each other, since we can assume that the invariant holds upon
        // entry to this method.)
        let mut existing_clauses = std::mem::take(&mut self.clauses).into_iter();
        for existing in existing_clauses.by_ref() {
            // Try to simplify the new clause against an existing clause.
            match existing.simplify_clauses(db, clause) {
                Simplifiable::NeverSatisfiable => {
                    // If two clauses cancel out to 0, that does NOT cause the entire set to become
                    // 0.  We need to keep whatever clauses have already been added to the result,
                    // and also need to copy over any later clauses that we hadn't processed yet.
                    self.clauses.extend(existing_clauses);
                    return;
                }

                Simplifiable::AlwaysSatisfiable => {
                    // If two clauses cancel out to 1, that makes the entire set 1, and all
                    // existing clauses are simplified away.
                    self.clauses.clear();
                    self.clauses.push(ConstraintClause::always());
                    return;
                }

                Simplifiable::NotSimplified(existing, c) => {
                    // We couldn't simplify the new clause relative to this existing clause, so add
                    // the existing clause to the result. Continue trying to simplify the new
                    // clause against the later existing clauses.
                    self.clauses.push(existing);
                    clause = c;
                }

                Simplifiable::Simplified(c) => {
                    // We were able to simplify the new clause relative to this existing clause.
                    // Don't add it to the result yet; instead, try to simplify the result further
                    // against later existing clauses.
                    clause = c;
                }
            }
        }

        // If we fall through then we need to add the new clause to the clause list (either because
        // we couldn't simplify it with anything, or because we did without it canceling out).
        self.clauses.push(clause);
    }

    /// Updates this set to be the union of itself and another set.
    fn union_set(&mut self, db: &'db dyn Db, other: Self) {
        for clause in other.clauses {
            self.union_clause(db, Satisfiable::Constrained(clause));
        }
    }

    /// Updates this set to be the intersection of itself and another set.
    fn intersect_set(&mut self, db: &'db dyn Db, other: &Self) {
        // This is the distributive law:
        // (A ∪ B) ∩ (C ∪ D ∪ E) = (A ∩ C) ∪ (A ∩ D) ∪ (A ∩ E) ∪ (B ∩ C) ∪ (B ∩ D) ∪ (B ∩ E)
        let self_clauses = std::mem::take(&mut self.clauses);
        for self_clause in &self_clauses {
            for other_clause in &other.clauses {
                self.union_clause(db, self_clause.intersect_clause(db, other_clause));
            }
        }
    }

    pub(crate) fn display(&self, db: &'db dyn Db) -> impl Display {
        struct DisplayConstraintSet<'a, 'db> {
            set: &'a ConstraintSet<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplayConstraintSet<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if self.set.clauses.is_empty() {
                    return f.write_str("0");
                }
                for (i, clause) in self.set.clauses.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ∨ ")?;
                    }
                    clause.display(self.db).fmt(f)?;
                }
                Ok(())
            }
        }

        DisplayConstraintSet { set: self, db }
    }
}

impl From<bool> for ConstraintSet<'_> {
    fn from(b: bool) -> Self {
        if b { Self::always() } else { Self::never() }
    }
}

/// The intersection of zero or more individual constraints.
///
/// This is called a "constraint set", and denoted _C_, in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct ConstraintClause<'db> {
    // NOTE: We use 1 here because most clauses only mention a single typevar.
    constraints: SmallVec<[ConstrainedTypeVar<'db>; 1]>,
}

impl<'db> ConstraintClause<'db> {
    fn new(constraints: SmallVec<[ConstrainedTypeVar<'db>; 1]>) -> Satisfiable<Self> {
        if constraints.is_empty() {
            Satisfiable::Always
        } else {
            Satisfiable::Constrained(Self { constraints })
        }
    }

    /// Returns the clause that is always satisfiable.
    fn always() -> Self {
        Self {
            constraints: smallvec![],
        }
    }

    /// Returns a clause containing a single constraint.
    fn singleton(constraint: ConstrainedTypeVar<'db>) -> Self {
        Self {
            constraints: smallvec![constraint],
        }
    }

    /// Returns whether this constraint is always satisfiable.
    fn is_always(&self) -> bool {
        self.constraints.is_empty()
    }

    fn is_satisfiable(&self) -> Satisfiable<()> {
        if self.is_always() {
            Satisfiable::Always
        } else {
            Satisfiable::Constrained(())
        }
    }

    /// Updates this clause to be the intersection of itself and an individual constraint. Returns
    /// a flag indicating whether the updated clause is never, always, or sometimes satisfied.
    fn intersect_constraint(
        &mut self,
        db: &'db dyn Db,
        constraint: Satisfiable<ConstrainedTypeVar<'db>>,
    ) -> Satisfiable<()> {
        let mut constraint = match constraint {
            // If the new constraint cannot be satisfied, that causes this whole clause to be
            // unsatisfiable too.
            Satisfiable::Never => return Satisfiable::Never,

            // If the new constraint can always satisfied, then the clause does not change. It was
            // not always satisfiable before, and so it still isn't.
            Satisfiable::Always => return Satisfiable::Constrained(()),

            Satisfiable::Constrained(constraint) => constraint,
        };

        // Naively, we would just append the new constraint to the clauses's list of constraints.
        // But that doesn't ensure that the constraints are simplified with respect to each other.
        // So instead, we iterate through the list of existing constraints, and try to simplify the
        // new constraint against each one in turn. (We can assume that the existing constraints
        // are already simplified with respect to each other, since we can assume that the
        // invariant holds upon entry to this method.)
        let mut existing_constraints = std::mem::take(&mut self.constraints).into_iter();
        for existing in existing_constraints.by_ref() {
            // Try to simplify the new constraint against an existing constraint.
            match existing.intersect(db, &constraint) {
                Some(Satisfiable::Never) => {
                    // If two constraints cancel out to 0, that makes the entire clause 0, and all
                    // existing constraints are simplified away.
                    return Satisfiable::Never;
                }

                Some(Satisfiable::Always) => {
                    // If two constraints cancel out to 1, that does NOT cause the entire clause to
                    // become 1. We need to keep whatever constraints have already been added to
                    // the result, and also need to copy over any later constraints that we hadn't
                    // processed yet.
                    self.constraints.extend(existing_constraints);
                    return self.is_satisfiable();
                }

                None => {
                    // We couldn't simplify the new constraint relative to this existing
                    // constraint, so add the existing constraint to the result. Continue trying to
                    // simplify the new constraint against the later existing constraints.
                    self.constraints.push(existing.clone());
                }

                Some(Satisfiable::Constrained(simplified)) => {
                    // We were able to simplify the new constraint relative to this existing
                    // constraint. Don't add it to the result yet; instead, try to simplify the
                    // result further against later existing constraints.
                    constraint = simplified;
                }
            }
        }

        // If we fall through then we need to add the new constraint to the constraint list (either
        // because we couldn't simplify it with anything, or because we did without it canceling
        // out).
        self.constraints.push(constraint);
        self.is_satisfiable()
    }

    /// Returns the intersection of this clause with another.
    fn intersect_clause(&self, db: &'db dyn Db, other: &Self) -> Satisfiable<Self> {
        // Add each `other` constraint in turn. Short-circuit if the result ever becomes 0.
        let mut result = self.clone();
        for constraint in &other.constraints {
            match result.intersect_constraint(db, Satisfiable::Constrained(constraint.clone())) {
                Satisfiable::Never => return Satisfiable::Never,
                Satisfiable::Always | Satisfiable::Constrained(()) => {}
            }
        }
        if result.is_always() {
            Satisfiable::Always
        } else {
            Satisfiable::Constrained(result)
        }
    }

    /// Tries to simplify the union of two clauses into a single clause, if possible.
    fn simplify_clauses(self, db: &'db dyn Db, other: Self) -> Simplifiable<Self> {
        // Saturation
        //
        // If either clause is always satisfiable, the union is too. (`1 ∪ C₂ = 1`, `C₁ ∪ 1 = 1`)
        //
        // ```py
        // class A[T]: ...
        //
        // class C1[U]:
        //     # T can specialize to any type, so this is "always satisfiable", or `1`
        //     x: A[U]
        //
        // class C2[V: int]:
        //     # `T ≤ int`
        //     x: A[V]
        //
        // class Saturation[U, V: int]:
        //     # `1 ∪ (T ≤ int)`
        //     # simplifies via saturation to
        //     # `T ≤ int`
        //     x: A[U] | A[V]
        // ```
        if self.is_always() || other.is_always() {
            return Simplifiable::Simplified(Self::always());
        }

        // Subsumption
        //
        // If either clause subsumes (is "smaller than") the other, then the union simplifies to
        // the "bigger" clause (the one being subsumed):
        //
        // - `A ∩ B` must be at least as large as `A ∩ B ∩ C`
        // - Therefore, `(A ∩ B) ∪ (A ∩ B ∩ C) = (A ∩ B)`
        //
        // (Note that possibly counterintuitively, "bigger" here means _fewer_ constraints in the
        // intersection, since intersecting more things can only make the result smaller.)
        //
        // ```py
        // class A[T, U, V]: ...
        //
        // class C1[X: int, Y: str, Z]:
        //     # `(T ≤ int ∩ U ≤ str)`
        //     x: A[X, Y, Z]
        //
        // class C2[X: int, Y: str, Z: bytes]:
        //     # `(T ≤ int ∩ U ≤ str ∩ V ≤ bytes)`
        //     x: A[X, Y, Z]
        //
        // class Subsumption[X1: int, Y1: str, Z2, X2: int, Y2: str, Z2: bytes]:
        //     # `(T ≤ int ∩ U ≤ str) ∪ (T ≤ int ∩ U ≤ str ∩ V ≤ bytes)`
        //     # simplifies via subsumption to
        //     # `(T ≤ int ∩ U ≤ str)`
        //     x: A[X1, Y1, Z2] | A[X2, Y2, Z2]
        // ```
        //
        // TODO: Consider checking both directions in one pass, possibly via a tri-valued return
        // value.
        if self.subsumes_via_intersection(db, &other) {
            return Simplifiable::Simplified(other);
        }
        if other.subsumes_via_intersection(db, &self) {
            return Simplifiable::Simplified(self);
        }

        // Distribution
        //
        // If the two clauses constrain the same typevar in an "overlapping" way, we can factor
        // that out:
        //
        // (A₁ ∩ B ∩ C) ∪ (A₂ ∩ B ∩ C) = (A₁ ∪ A₂) ∩ B ∩ C
        //
        // ```py
        // class A[T, U, V]: ...
        //
        // class C1[X: int, Y: str, Z: bytes]:
        //     # `(T ≤ int ∩ U ≤ str ∩ V ≤ bytes)`
        //     x: A[X, Y, Z]
        //
        // class C2[X: bool, Y: str, Z: bytes]:
        //     # `(T ≤ bool ∩ U ≤ str ∩ V ≤ bytes)`
        //     x: A[X, Y, Z]
        //
        // class Distribution[X1: int, Y1: str, Z2: bytes, X2: bool, Y2: str, Z2: bytes]:
        //     # `(T ≤ int ∩ U ≤ str ∩ V ≤ bytes) ∪ (T ≤ bool ∩ U ≤ str ∩ V ≤ bytes)`
        //     # simplifies via distribution to
        //     # `(T ≤ int ∪ T ≤ bool) ∩ U ≤ str ∩ V ≤ bytes)`
        //     # which (because `bool ≤ int`) is equivalent to
        //     # `(T ≤ int ∩ U ≤ str ∩ V ≤ bytes)`
        //     x: A[X1, Y1, Z2] | A[X2, Y2, Z2]
        // ```
        if let Some(simplified) = self.simplifies_via_distribution(db, &other) {
            return simplified;
        }

        // Can't be simplified
        Simplifiable::NotSimplified(self, other)
    }

    /// Returns whether this clause subsumes `other` via intersection — that is, if the
    /// intersection of `self` and `other` is `self`.
    fn subsumes_via_intersection(&self, db: &'db dyn Db, other: &Self) -> bool {
        // See the notes in `simplify_clauses` for more details on subsumption, including Python
        // examples that cause it to fire.

        if self.constraints.len() != other.constraints.len() {
            return false;
        }

        let pairwise = (self.constraints.iter())
            .merge_join_by(&other.constraints, |a, b| a.typevar.cmp(&b.typevar));
        for pair in pairwise {
            match pair {
                // `other` contains a constraint whose typevar doesn't appear in `self`, so `self`
                // cannot be smaller.
                EitherOrBoth::Right(_) => return false,

                // `self` contains a constraint whose typevar doesn't appear in `other`. `self`
                // might be smaller, but we still have to check the remaining constraints.
                EitherOrBoth::Left(_) => continue,

                // Both clauses contain a constraint with this typevar; verify that the constraint
                // in `self` is smaller.
                EitherOrBoth::Both(self_constraint, other_constraint) => {
                    if !self_constraint.subsumes(db, other_constraint) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// If the union of two clauses is simpler than either of the individual clauses, returns the
    /// union. This happens when they mention the same set of typevars and the constraints for all
    /// but one typevar are identical. Moreover, for the other typevar, the union of the
    /// constraints for that typevar simplifies to (a) a single constraint, or (b) two constraints
    /// where one of them is smaller than before. That is,
    ///
    /// ```text
    /// (A₁ ∩ B ∩ C) ∪ (A₂ ∩ B ∩ C) = A₁₂ ∩ B ∩ C
    ///                            or (A₁' ∪ A₂) ∩ B ∩ C
    ///                            or (A₁ ∪ A₂') ∩ B ∩ C
    /// ```
    ///
    /// where `B` and `C` are the constraints that are identical for all but one typevar, and `A₁`
    /// and `A₂` are the constraints for the other typevar; and where `A₁ ∪ A₂` either simplifies
    /// to a single constraint (`A₁₂`), or to a union where either `A₁` or `A₂` becomes smaller
    /// (`A₁'` or `A₂'`, respectively).
    ///
    /// Otherwise returns `None`.
    fn simplifies_via_distribution(
        &self,
        db: &'db dyn Db,
        other: &Self,
    ) -> Option<Simplifiable<Self>> {
        // See the notes in `simplify_clauses` for more details on distribution, including Python
        // examples that cause it to fire.

        if self.constraints.len() != other.constraints.len() {
            return None;
        }

        // Verify that the constraints for precisely one typevar simplify, and the constraints for
        // all other typevars are identical. Remember the index of the typevar whose constraints
        // simplify.
        let mut simplified_index = None;
        let pairwise = (self.constraints.iter())
            .merge_join_by(&other.constraints, |a, b| a.typevar.cmp(&b.typevar));
        for (index, pair) in pairwise.enumerate() {
            match pair {
                // If either clause contains a constraint whose typevar doesn't appear in the
                // other, the clauses don't simplify.
                EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => return None,

                EitherOrBoth::Both(self_constraint, other_constraint) => {
                    if self_constraint == other_constraint {
                        continue;
                    }
                    let Some(union_constraint) =
                        self_constraint.simplified_union(db, other_constraint)
                    else {
                        // The constraints for this typevar are not identical, nor do they
                        // simplify.
                        return None;
                    };
                    if simplified_index
                        .replace((index, union_constraint))
                        .is_some()
                    {
                        // More than one constraint simplify, which doesn't allow the clause as a
                        // whole to simplify.
                        return None;
                    }
                }
            }
        }

        let Some((index, union_constraint)) = simplified_index else {
            // We never found a typevar whose constraints simplify.
            return None;
        };
        let mut constraints = self.constraints.clone();
        match union_constraint {
            Simplifiable::NeverSatisfiable => {
                panic!("unioning two non-never constraints should not be never")
            }
            Simplifiable::AlwaysSatisfiable => {
                // If the simplified union of constraints is Always, then we can remove this typevar
                // from the constraint completely.
                constraints.remove(index);
                Some(Simplifiable::from_one(Self::new(constraints)))
            }
            Simplifiable::Simplified(union_constraint) => {
                constraints[index] = union_constraint;
                Some(Simplifiable::from_one(Self::new(constraints)))
            }
            Simplifiable::NotSimplified(left, right) => {
                let mut left_constraints = constraints.clone();
                let mut right_constraints = constraints;
                left_constraints[index] = left;
                right_constraints[index] = right;
                Some(Simplifiable::from_union(
                    Self::new(left_constraints),
                    Self::new(right_constraints),
                ))
            }
        }
    }

    /// Returns the negation of this clause. The result is a set since negating an intersection
    /// produces a union.
    fn negate(&self, db: &'db dyn Db) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::never();
        for constraint in &self.constraints {
            constraint.negate_into(db, &mut result);
        }
        result
    }

    fn display(&self, db: &'db dyn Db) -> impl Display {
        struct DisplayConstraintClause<'a, 'db> {
            clause: &'a ConstraintClause<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplayConstraintClause<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if self.clause.constraints.is_empty() {
                    return f.write_str("1");
                }

                let clause_count: usize = (self.clause.constraints.iter())
                    .map(ConstrainedTypeVar::clause_count)
                    .sum();
                if clause_count > 1 {
                    f.write_str("(")?;
                }
                for (i, constraint) in self.clause.constraints.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ∧ ")?;
                    }
                    constraint.display(self.db).fmt(f)?;
                }
                if clause_count > 1 {
                    f.write_str(")")?;
                }
                Ok(())
            }
        }

        DisplayConstraintClause { clause: self, db }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct ConstrainedTypeVar<'db> {
    typevar: BoundTypeVarInstance<'db>,
    constraint: Constraint<'db>,
}

impl<'db> ConstrainedTypeVar<'db> {
    fn clause_count(&self) -> usize {
        self.constraint.clause_count()
    }

    /// Returns the intersection of this individual constraint and another, or `None` if the two
    /// constraints do not refer to the same typevar (and therefore cannot be simplified to a
    /// single constraint).
    fn intersect(&self, db: &'db dyn Db, other: &Self) -> Option<Satisfiable<Self>> {
        if self.typevar != other.typevar {
            return None;
        }
        Some(
            self.constraint
                .intersect(db, &other.constraint)
                .map(|constraint| constraint.constrain(self.typevar)),
        )
    }

    /// Returns the union of this individual constraint and another, if it can be simplified to a
    /// union of two constraints or fewer. Returns `None` if the union cannot be simplified that
    /// much.
    fn simplified_union(&self, db: &'db dyn Db, other: &Self) -> Option<Simplifiable<Self>> {
        if self.typevar != other.typevar {
            return None;
        }
        self.constraint
            .simplified_union(db, &other.constraint)
            .map(|constraint| constraint.map(|constraint| constraint.constrain(self.typevar)))
    }

    /// Adds the negation of this individual constraint to a constraint set.
    fn negate_into(&self, db: &'db dyn Db, set: &mut ConstraintSet<'db>) {
        self.constraint.negate_into(db, self.typevar, set);
    }

    /// Returns whether `self` has tighter bounds than `other` — that is, if the intersection of
    /// `self` and `other` is `self`.
    fn subsumes(&self, db: &'db dyn Db, other: &Self) -> bool {
        debug_assert_eq!(self.typevar, other.typevar);
        match self.intersect(db, other) {
            Some(Satisfiable::Constrained(intersection)) => intersection == *self,
            _ => false,
        }
    }

    fn display(&self, db: &'db dyn Db) -> impl Display {
        self.constraint.display(db, self.typevar.display(db))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct Constraint<'db> {
    positive: RangeConstraint<'db>,
    negative: SmallVec<[NegatedRangeConstraint<'db>; 1]>,
}

impl<'db> Constraint<'db> {
    fn constrain(self, typevar: BoundTypeVarInstance<'db>) -> ConstrainedTypeVar<'db> {
        ConstrainedTypeVar {
            typevar,
            constraint: self,
        }
    }

    fn clause_count(&self) -> usize {
        usize::from(!self.positive.is_always()) + self.negative.len()
    }

    fn satisfiable(self, db: &'db dyn Db) -> Satisfiable<Self> {
        if self.positive.is_always() && self.negative.is_empty() {
            return Satisfiable::Always;
        }
        if (self.negative.iter()).any(|negative| negative.hole.contains(db, &self.positive)) {
            return Satisfiable::Never;
        }
        Satisfiable::Constrained(self)
    }

    fn intersect(&self, db: &'db dyn Db, other: &Constraint<'db>) -> Satisfiable<Constraint<'db>> {
        let Some(positive) = self.positive.intersect(db, &other.positive) else {
            // If the positive intersection is empty, none of the negative holes matter, since
            // there are no types for the holes to remove.
            return Satisfiable::Never;
        };

        // The negative portion of the intersection is given by
        //
        //   ¬(s₁ ≤ α ≤ t₁) ∧ ¬(s₂ ≤ α ≤ t₂) = ¬((s₁ ≤ α ≤ t₁) ∨ (s₂ ≤ α ≤ t₂))
        //
        // That is, we union together the holes from `self` and `other`. If any of the holes
        // entirely contain another, we can simplify those two down to the larger hole. We use the
        // same trick as above in `union_clause` and `intersect_constraint` to look for pairs that
        // we can simplify.
        //
        // We also want to clip each negative hole to the minimum range that overlaps with the
        // positive range. We'll do that now to all of the holes from `self`, and we'll do that to
        // holes from `other` below when we try to simplify them.
        let mut previous: SmallVec<[NegatedRangeConstraint<'db>; 1]> = SmallVec::new();
        let mut current: SmallVec<_> = (self.negative.iter())
            .filter_map(|negative| negative.clip_to_positive(db, &positive))
            .collect();
        for other_negative in &other.negative {
            let Some(mut other_negative) = other_negative.clip_to_positive(db, &positive) else {
                continue;
            };
            std::mem::swap(&mut previous, &mut current);
            let mut previous_negative = previous.iter();
            for self_negative in previous_negative.by_ref() {
                match self_negative.intersect_negative(db, &other_negative) {
                    None => {
                        // We couldn't simplify the new hole relative to this existing holes, so
                        // add the existing hole to the result. Continue trying to simplify the new
                        // hole against the later existing holes.
                        current.push(self_negative.clone());
                    }

                    Some(union) => {
                        // We were able to simplify the new hole relative to this existing hole.
                        // Don't add it to the result yet; instead, try to simplify the result
                        // further against later existing holes.
                        other_negative = union.clone();
                    }
                }
            }

            // If we fall through then we need to add the new hole to the hole list (either because
            // we couldn't simplify it with anything, or because we did without it canceling out).
            current.push(other_negative);
        }

        let result = Self {
            positive,
            negative: current,
        };
        result.satisfiable(db)
    }

    fn simplified_union(
        &self,
        db: &'db dyn Db,
        other: &Constraint<'db>,
    ) -> Option<Simplifiable<Constraint<'db>>> {
        // (ap ∧ ¬an₁ ∧ ¬an₂ ∧ ...) ∨ (bp ∧ ¬bn₁ ∧ ¬bn₂ ∧ ...)
        //   = (ap ∨ bp) ∧ (ap ∨ ¬bn₁) ∧ (ap ∨ ¬bn₂) ∧ ...
        //   ∧ (¬an₁ ∨ bp) ∧ (¬an₁ ∨ ¬bn₁) ∧ (¬an₁ ∨ ¬bn₂) ∧ ...
        //   ∧ (¬an₂ ∨ bp) ∧ (¬an₂ ∨ ¬bn₁) ∧ (¬an₂ ∨ ¬bn₂) ∧ ...
        //
        // We use a helper type to build up the result of the union of two constraints, since we
        // need to calculate the Cartesian product of the the positive and negative portions of the
        // two inputs. We cannot use `ConstraintSet` for this, since it would try to invoke the
        // `simplify_union` logic, which this method is part of the definition of! So we have to
        // reproduce some of that logic here, in a simplified form since we know we're only ever
        // looking at pairs of individual constraints at a time.

        struct Results<'db> {
            next: Vec<Constraint<'db>>,
            results: Vec<Constraint<'db>>,
        }

        impl<'db> Results<'db> {
            fn new(constraint: Constraint<'db>) -> Results<'db> {
                Results {
                    next: vec![],
                    results: vec![constraint],
                }
            }

            fn flip(&mut self) {
                std::mem::swap(&mut self.next, &mut self.results);
                self.next.clear();
            }

            /// Adds a constraint by intersecting it with any currently pending results.
            fn add_constraint(&mut self, db: &'db dyn Db, constraint: &Constraint<'db>) {
                self.next.extend(self.results.iter().filter_map(|result| {
                    match result.intersect(db, constraint) {
                        Satisfiable::Never => None,
                        Satisfiable::Always => Some(Constraint {
                            positive: RangeConstraint::always(),
                            negative: smallvec![],
                        }),
                        Satisfiable::Constrained(constraint) => Some(constraint),
                    }
                }));
            }

            /// Adds a single negative range constraint to the pending results.
            fn add_negated_range(
                &mut self,
                db: &'db dyn Db,
                negative: Option<NegatedRangeConstraint<'db>>,
            ) {
                let negative = match negative {
                    Some(negative) => Constraint {
                        positive: RangeConstraint::always(),
                        negative: smallvec![negative],
                    },
                    // If the intersection of these two holes is empty, then they don't remove
                    // anything from the final union.
                    None => return,
                };
                self.add_constraint(db, &negative);
                self.flip();
            }

            /// Adds a possibly simplified constraint to the pending results. If the parameter has
            /// been simplified to a single constraint, it is intersected with each currently
            /// pending result. If it could not be simplified (i.e., it is the union of two
            /// constraints), then we duplicate any pending results, so that we can _separately_
            /// intersect each non-simplified constraint with the results.
            fn add_simplified_constraint(
                &mut self,
                db: &'db dyn Db,
                constraints: Simplifiable<Constraint<'db>>,
            ) {
                match constraints {
                    Simplifiable::NeverSatisfiable => {
                        self.results.clear();
                        return;
                    }
                    Simplifiable::AlwaysSatisfiable => {
                        return;
                    }
                    Simplifiable::Simplified(constraint) => {
                        self.add_constraint(db, &constraint);
                    }
                    Simplifiable::NotSimplified(first, second) => {
                        self.add_constraint(db, &first);
                        self.add_constraint(db, &second);
                    }
                }
                self.flip();
            }

            /// If there are two or fewer final results, translates them into a [`Simplifiable`]
            /// result. Otherwise returns `None`, indicating that the union cannot be simplified
            /// enough for our purposes.
            fn into_result(self, db: &'db dyn Db) -> Option<Simplifiable<Constraint<'db>>> {
                let mut results = self.results.into_iter();
                let Some(first) = results.next() else {
                    return Some(Simplifiable::NeverSatisfiable);
                };
                let Some(second) = results.next() else {
                    return Some(Simplifiable::from_one(first.satisfiable(db)));
                };
                if results.next().is_some() {
                    return None;
                }
                Some(Simplifiable::from_union(
                    first.satisfiable(db),
                    second.satisfiable(db),
                ))
            }
        }

        let mut results = match self.positive.union(db, &other.positive) {
            Some(positive) => Results::new(Constraint {
                positive: positive.clone(),
                negative: smallvec![],
            }),
            None => return None,
        };
        for other_negative in &other.negative {
            results.add_simplified_constraint(
                db,
                self.positive.union_negated_range(db, other_negative),
            );
        }
        for self_negative in &self.negative {
            // Reverse the results here so that we always add items from `self` first. This ensures
            // that the output we produce is ordered consistently with the input we receive.
            results.add_simplified_constraint(
                db,
                other
                    .positive
                    .union_negated_range(db, self_negative)
                    .reverse(),
            );
        }
        for self_negative in &self.negative {
            for other_negative in &other.negative {
                results.add_negated_range(db, self_negative.union_negative(db, other_negative));
            }
        }

        results.into_result(db)
    }

    fn negate_into(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        set: &mut ConstraintSet<'db>,
    ) {
        for negative in &self.negative {
            set.union_constraint(
                db,
                Constraint::range(db, negative.hole.lower, negative.hole.upper).constrain(typevar),
            );
        }
        set.union_constraint(
            db,
            Constraint::negated_range(db, self.positive.lower, self.positive.upper)
                .constrain(typevar),
        );
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayConstraint<'a, 'db, D> {
            constraint: &'a Constraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut first = true;
                if !self.constraint.positive.is_always() {
                    (self.constraint.positive)
                        .display(self.db, &self.typevar)
                        .fmt(f)?;
                    first = false;
                }
                for negative in &self.constraint.negative {
                    if first {
                        first = false;
                    } else {
                        f.write_str(" ∧ ")?;
                    }
                    negative.display(self.db, &self.typevar).fmt(f)?;
                }
                Ok(())
            }
        }

        DisplayConstraint {
            constraint: self,
            typevar,
            db,
        }
    }
}

impl<'db> Satisfiable<Constraint<'db>> {
    fn constrain(self, typevar: BoundTypeVarInstance<'db>) -> Satisfiable<ConstrainedTypeVar<'db>> {
        self.map(|constraint| constraint.constrain(typevar))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct RangeConstraint<'db> {
    lower: Type<'db>,
    upper: Type<'db>,
}

impl<'db> Constraint<'db> {
    /// Returns a new range constraint.
    ///
    /// Panics if `lower` and `upper` are not both fully static.
    fn range(db: &'db dyn Db, lower: Type<'db>, upper: Type<'db>) -> Satisfiable<Constraint<'db>> {
        debug_assert_eq!(lower, lower.bottom_materialization(db));
        debug_assert_eq!(upper, upper.top_materialization(db));

        // If `lower ≰ upper`, then the constraint cannot be satisfied, since there is no type that
        // is both greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return Satisfiable::Never;
        }

        // If the requested constraint is `Never ≤ T ≤ object`, then the typevar can be specialized
        // to _any_ type, and the constraint does nothing.
        let positive = RangeConstraint { lower, upper };
        if positive.is_always() {
            return Satisfiable::Always;
        }

        Satisfiable::Constrained(Constraint {
            positive,
            negative: smallvec![],
        })
    }
}

impl<'db> RangeConstraint<'db> {
    fn always() -> Self {
        Self {
            lower: Type::Never,
            upper: Type::object(),
        }
    }

    fn contains(&self, db: &'db dyn Db, other: &RangeConstraint<'db>) -> bool {
        self.lower.is_subtype_of(db, other.lower) && other.upper.is_subtype_of(db, self.upper)
    }

    fn is_always(&self) -> bool {
        self.lower.is_never() && self.upper.is_object()
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect(&self, db: &'db dyn Db, other: &RangeConstraint<'db>) -> Option<Self> {
        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = UnionType::from_elements(db, [self.lower, other.lower]);
        let upper = IntersectionType::from_elements(db, [self.upper, other.upper]);

        // If `lower ≰ upper`, then the intersection is empty, since there is no type that is both
        // greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return None;
        }

        Some(Self { lower, upper })
    }

    /// Returns the union of two range constraints if it can be simplified to a single constraint.
    /// Otherwise returns `None`.
    fn union(&self, db: &'db dyn Db, other: &RangeConstraint<'db>) -> Option<Self> {
        // When one of the bounds is entirely contained within the other, the union simplifies to
        // the larger bounds.
        if self.lower.is_subtype_of(db, other.lower) && other.upper.is_subtype_of(db, self.upper) {
            return Some(self.clone());
        }
        if other.lower.is_subtype_of(db, self.lower) && self.upper.is_subtype_of(db, other.upper) {
            return Some(other.clone());
        }

        // Otherwise the result cannot be simplified.
        None
    }

    /// Returns the union of a positive range with a negative hole.
    fn union_negated_range(
        &self,
        db: &'db dyn Db,
        negated: &NegatedRangeConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // If the positive range completely contains the negative range, then the union is always
        // satisfied.
        if self.contains(db, &negated.hole) {
            return Simplifiable::AlwaysSatisfiable;
        }

        // If the positive range is disjoint from the negative range, the positive range doesn't
        // add anything; the union is the negative range.
        if incomparable(db, self.lower, negated.hole.upper)
            || incomparable(db, negated.hole.lower, self.upper)
        {
            return Simplifiable::from_one(Constraint::negated_range(
                db,
                negated.hole.lower,
                negated.hole.upper,
            ));
        }

        // Otherwise we clip the positive constraint to the minimum range that overlaps with the
        // negative range.
        Simplifiable::from_union(
            Constraint::range(
                db,
                UnionType::from_elements(db, [self.lower, negated.hole.lower]),
                IntersectionType::from_elements(db, [self.upper, negated.hole.upper]),
            ),
            Constraint::negated_range(db, negated.hole.lower, negated.hole.upper),
        )
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayRangeConstraint<'a, 'db, D> {
            constraint: &'a RangeConstraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayRangeConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if (self.constraint.lower).is_equivalent_to(self.db, self.constraint.upper) {
                    return write!(
                        f,
                        "({} = {})",
                        &self.typevar,
                        self.constraint.lower.display(self.db)
                    );
                }

                f.write_str("(")?;
                if !self.constraint.lower.is_never() {
                    write!(f, "{} ≤ ", self.constraint.lower.display(self.db))?;
                }
                self.typevar.fmt(f)?;
                if !self.constraint.upper.is_object() {
                    write!(f, " ≤ {}", self.constraint.upper.display(self.db))?;
                }
                f.write_str(")")
            }
        }

        DisplayRangeConstraint {
            constraint: self,
            typevar,
            db,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct NegatedRangeConstraint<'db> {
    hole: RangeConstraint<'db>,
}

impl<'db> Constraint<'db> {
    /// Returns a new negated range constraint.
    ///
    /// Panics if `lower` and `upper` are not both fully static.
    fn negated_range(
        db: &'db dyn Db,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Satisfiable<Constraint<'db>> {
        debug_assert_eq!(lower, lower.bottom_materialization(db));
        debug_assert_eq!(upper, upper.top_materialization(db));

        // If `lower ≰ upper`, then the negated constraint is always satisfied, since there is no
        // type that is both greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return Satisfiable::Always;
        }

        // If the requested constraint is `¬(Never ≤ T ≤ object)`, then the constraint cannot be
        // satisfied.
        let negative = NegatedRangeConstraint {
            hole: RangeConstraint { lower, upper },
        };
        if negative.hole.is_always() {
            return Satisfiable::Never;
        }

        Satisfiable::Constrained(Constraint {
            positive: RangeConstraint::always(),
            negative: smallvec![negative],
        })
    }
}

impl<'db> NegatedRangeConstraint<'db> {
    /// Clips this negative hole to be the smallest hole that removes the same types from the given
    /// positive range.
    fn clip_to_positive(&self, db: &'db dyn Db, positive: &RangeConstraint<'db>) -> Option<Self> {
        self.hole
            .intersect(db, positive)
            .map(|hole| NegatedRangeConstraint { hole })
    }

    /// Returns the union of two negative constraints. (This this is _intersection_ of the
    /// constraints' holes.)
    fn union_negative(
        &self,
        db: &'db dyn Db,
        positive: &NegatedRangeConstraint<'db>,
    ) -> Option<Self> {
        self.hole
            .intersect(db, &positive.hole)
            .map(|hole| NegatedRangeConstraint { hole })
    }

    /// Returns the intersection of two negative constraints. (This this is _union_ of the
    /// constraints' holes.)
    fn intersect_negative(
        &self,
        db: &'db dyn Db,
        other: &NegatedRangeConstraint<'db>,
    ) -> Option<Self> {
        self.hole
            .union(db, &other.hole)
            .map(|hole| NegatedRangeConstraint { hole })
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayNegatedRangeConstraint<'a, 'db, D> {
            constraint: &'a NegatedRangeConstraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayNegatedRangeConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if (self.constraint.hole.lower)
                    .is_equivalent_to(self.db, self.constraint.hole.upper)
                {
                    return write!(
                        f,
                        "({} ≠ {})",
                        &self.typevar,
                        self.constraint.hole.lower.display(self.db)
                    );
                }

                f.write_str("¬")?;
                self.constraint.hole.display(self.db, &self.typevar).fmt(f)
            }
        }

        DisplayNegatedRangeConstraint {
            constraint: self,
            typevar,
            db,
        }
    }
}

/// Wraps a constraint (or clause, or set), while using distinct variants to represent when the
/// constraint is never satisfiable or always satisfiable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Satisfiable<T> {
    Never,
    Always,
    Constrained(T),
}

impl<T> Satisfiable<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> Satisfiable<U> {
        match self {
            Satisfiable::Never => Satisfiable::Never,
            Satisfiable::Always => Satisfiable::Always,
            Satisfiable::Constrained(t) => Satisfiable::Constrained(f(t)),
        }
    }
}

/// The result of trying to simplify two constraints (or clauses, or sets). Like [`Satisfiable`],
/// we use distinct variants to represent when the simplification is never satisfiable or always
/// satisfiable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Simplifiable<T> {
    NeverSatisfiable,
    AlwaysSatisfiable,
    Simplified(T),
    NotSimplified(T, T),
}

impl<T> Simplifiable<T> {
    fn from_one(constraint: Satisfiable<T>) -> Self {
        match constraint {
            Satisfiable::Never => Simplifiable::NeverSatisfiable,
            Satisfiable::Always => Simplifiable::AlwaysSatisfiable,
            Satisfiable::Constrained(constraint) => Simplifiable::Simplified(constraint),
        }
    }
}

impl<T> Simplifiable<T> {
    fn from_union(first: Satisfiable<T>, second: Satisfiable<T>) -> Self {
        match (first, second) {
            (Satisfiable::Always, _) | (_, Satisfiable::Always) => Simplifiable::AlwaysSatisfiable,
            (Satisfiable::Never, Satisfiable::Never) => Simplifiable::NeverSatisfiable,
            (Satisfiable::Never, Satisfiable::Constrained(constraint))
            | (Satisfiable::Constrained(constraint), Satisfiable::Never) => {
                Simplifiable::Simplified(constraint)
            }
            (Satisfiable::Constrained(first), Satisfiable::Constrained(second)) => {
                Simplifiable::NotSimplified(first, second)
            }
        }
    }

    fn map<U>(self, mut f: impl FnMut(T) -> U) -> Simplifiable<U> {
        match self {
            Simplifiable::NeverSatisfiable => Simplifiable::NeverSatisfiable,
            Simplifiable::AlwaysSatisfiable => Simplifiable::AlwaysSatisfiable,
            Simplifiable::Simplified(t) => Simplifiable::Simplified(f(t)),
            Simplifiable::NotSimplified(t1, t2) => Simplifiable::NotSimplified(f(t1), f(t2)),
        }
    }

    fn reverse(self) -> Self {
        match self {
            Simplifiable::NeverSatisfiable
            | Simplifiable::AlwaysSatisfiable
            | Simplifiable::Simplified(_) => self,
            Simplifiable::NotSimplified(t1, t2) => Simplifiable::NotSimplified(t2, t1),
        }
    }
}
