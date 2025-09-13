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
//! There are three possible individual constraints:
//!
//! - A _range_ constraint requires the typevar to be within a particular lower and upper bound:
//!   the typevar can only specialize to a type that is a supertype of the lower bound, and a
//!   subtype of the upper bound.
//!
//! - A _not-equivalent_ constraint requires the typevar to specialize to anything _other_ than a
//!   particular type (the "hole").
//!
//! - An _incomparable_ constraint requires the typevar to specialize to any type that is neither a
//!   subtype nor a supertype of a particular type (the "pivot").
//!
//! Not-equivalent and incomparable constraints are usually not constructed directly; instead, they
//! typically arise when building up complex combinations of range constraints.
//!
//! Note that all of the types that a constraint compares against — the bounds of a range
//! constraint, the hole of not-equivalent constraint, and the pivot of an incomparable constraint
//! — must be fully static. We take the bottom and top materializations of the types to remove any
//! gradual forms if needed.
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
//!
//! Python does not give us an easy way to construct this, but we can also consider a typevar that
//! can specialize to any type that `T` _cannot_ specialize to — that is, the negation of `T`'s
//! constraint. Another way to write `Never ≤ V ≤ B` is `Never ≤ V ∩ V ≤ B`; if we negate that, we
//! get `¬(Never ≤ V) ∪ ¬(V ≤ B)`, or
//!
//! ```text
//! ((V ≤ Never ∩ V ≠ Never) ∪ V ≁ Never) ∪ ((B ≤ V ∩ V ≠ B) ∪ V ≁ B)
//! ```
//!
//! The first constraint in the union indicates that `V` can specialize to any type that is a
//! subtype of `Never` or incomparable with `Never`, but not to `Never` itself.
//!
//! The second constraint in the union indicates that `V` can specialize to any type that is a
//! supertype of `B` or incomparable with `B`, but not to `B` itself. (For instance, it _can_
//! specialize to `A`.)
//!
//! There aren't any types that satisfy the first constraint in the union (the type would have to
//! somehow contain a negative number of values). You can think of a constraint that cannot be
//! satisfied as an empty set (of types), which means we can simplify it out of the union. That
//! gives us a final constraint of `(B ≤ V ∩ V ≠ B) ∪ V ≁ B` for the negation of `T`'s constraint.

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

    pub(crate) fn not_equivalent(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        hole: Type<'db>,
    ) -> Self {
        let constraint = Constraint::not_equivalent(db, hole).constrain(typevar);
        let mut result = Self::never();
        result.union_constraint(db, constraint);
        result
    }

    pub(crate) fn incomparable(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        pivot: Type<'db>,
    ) -> Self {
        let constraint = Constraint::incomparable(db, pivot).constrain(typevar);
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

    /// Returns a clause that is the intersection of an iterator of constraints.
    fn from_constraints(
        db: &'db dyn Db,
        constraints: impl IntoIterator<Item = Satisfiable<ConstrainedTypeVar<'db>>>,
    ) -> Satisfiable<Self> {
        let mut result = Self::always();
        for constraint in constraints {
            if result.intersect_constraint(db, constraint) == Satisfiable::Never {
                return Satisfiable::Never;
            }
        }
        if result.is_always() {
            return Satisfiable::Always;
        }
        Satisfiable::Constrained(result)
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
                Simplifiable::NeverSatisfiable => {
                    // If two constraints cancel out to 0, that makes the entire clause 0, and all
                    // existing constraints are simplified away.
                    return Satisfiable::Never;
                }

                Simplifiable::AlwaysSatisfiable => {
                    // If two constraints cancel out to 1, that does NOT cause the entire clause to
                    // become 1. We need to keep whatever constraints have already been added to
                    // the result, and also need to copy over any later constraints that we hadn't
                    // processed yet.
                    self.constraints.extend(existing_constraints);
                    return self.is_satisfiable();
                }

                Simplifiable::NotSimplified(existing, c) => {
                    // We couldn't simplify the new constraint relative to this existing
                    // constraint, so add the existing constraint to the result. Continue trying to
                    // simplify the new constraint against the later existing constraints.
                    self.constraints.push(existing);
                    constraint = c;
                }

                Simplifiable::Simplified(c) => {
                    // We were able to simplify the new constraint relative to this existing
                    // constraint. Don't add it to the result yet; instead, try to simplify the
                    // result further against later existing constraints.
                    constraint = c;
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
            if simplified.is_always() {
                return Simplifiable::AlwaysSatisfiable;
            }
            return Simplifiable::Simplified(simplified);
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
    /// union. This happens when (a) they mention the same set of typevars, (b) the union of the
    /// constraints for exactly one typevar simplifies to a single constraint, and (c) the
    /// constraints for all other typevars are identical. Otherwise returns `None`.
    fn simplifies_via_distribution(&self, db: &'db dyn Db, other: &Self) -> Option<Self> {
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
                    let union_constraint = match self_constraint.union(db, other_constraint) {
                        Simplifiable::NotSimplified(_, _) => {
                            // The constraints for this typevar are not identical, nor do they
                            // simplify.
                            return None;
                        }
                        Simplifiable::Simplified(union_constraint) => Some(union_constraint),
                        Simplifiable::AlwaysSatisfiable => None,
                        Simplifiable::NeverSatisfiable => {
                            panic!("unioning two non-never constraints should not be never")
                        }
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
        if let Some(union_constraint) = union_constraint {
            constraints[index] = union_constraint;
        } else {
            // If the simplified union of constraints is Always, then we can remove this typevar
            // from the constraint completely.
            constraints.remove(index);
        }
        Some(Self { constraints })
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

                if self.clause.constraints.len() > 1 {
                    f.write_str("(")?;
                }
                for (i, constraint) in self.clause.constraints.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ∧ ")?;
                    }
                    constraint.display(self.db).fmt(f)?;
                }
                if self.clause.constraints.len() > 1 {
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
    /// Returns the intersection of this individual constraint and another.
    fn intersect(&self, db: &'db dyn Db, other: &Self) -> Simplifiable<Self> {
        if self.typevar != other.typevar {
            return Simplifiable::NotSimplified(self.clone(), other.clone());
        }
        self.constraint
            .intersect(db, &other.constraint)
            .map(|constraint| constraint.constrain(self.typevar))
    }

    /// Returns the union of this individual constraint and another.
    fn union(&self, db: &'db dyn Db, other: &Self) -> Simplifiable<Self> {
        if self.typevar != other.typevar {
            return Simplifiable::NotSimplified(self.clone(), other.clone());
        }
        self.constraint
            .union(db, &other.constraint)
            .map(|constraint| constraint.constrain(self.typevar))
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
            Simplifiable::Simplified(intersection) => intersection == *self,
            _ => false,
        }
    }

    fn display(&self, db: &'db dyn Db) -> impl Display {
        self.constraint.display(db, self.typevar.display(db))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) enum Constraint<'db> {
    Range(RangeConstraint<'db>),
    NotEquivalent(NotEquivalentConstraint<'db>),
    Incomparable(IncomparableConstraint<'db>),
}

impl<'db> Constraint<'db> {
    fn constrain(self, typevar: BoundTypeVarInstance<'db>) -> ConstrainedTypeVar<'db> {
        ConstrainedTypeVar {
            typevar,
            constraint: self,
        }
    }

    fn intersect(&self, db: &'db dyn Db, other: &Constraint<'db>) -> Simplifiable<Constraint<'db>> {
        match (self, other) {
            (Constraint::Range(left), Constraint::Range(right)) => left.intersect_range(db, right),

            (Constraint::Range(range), Constraint::NotEquivalent(not_equivalent))
            | (Constraint::NotEquivalent(not_equivalent), Constraint::Range(range)) => {
                range.intersect_not_equivalent(db, not_equivalent)
            }

            (Constraint::Range(range), Constraint::Incomparable(incomparable))
            | (Constraint::Incomparable(incomparable), Constraint::Range(range)) => {
                range.intersect_incomparable(db, incomparable)
            }

            (Constraint::NotEquivalent(left), Constraint::NotEquivalent(right)) => {
                left.intersect_not_equivalent(db, right)
            }

            (Constraint::NotEquivalent(not_equivalent), Constraint::Incomparable(incomparable))
            | (Constraint::Incomparable(incomparable), Constraint::NotEquivalent(not_equivalent)) => {
                not_equivalent.intersect_incomparable(db, incomparable)
            }

            (Constraint::Incomparable(left), Constraint::Incomparable(right)) => {
                left.intersect_incomparable(db, right)
            }
        }
    }

    fn union(&self, db: &'db dyn Db, other: &Constraint<'db>) -> Simplifiable<Constraint<'db>> {
        match (self, other) {
            (Constraint::Range(left), Constraint::Range(right)) => left.union_range(db, right),

            (Constraint::Range(range), Constraint::NotEquivalent(not_equivalent))
            | (Constraint::NotEquivalent(not_equivalent), Constraint::Range(range)) => {
                range.union_not_equivalent(db, not_equivalent)
            }

            (Constraint::Range(range), Constraint::Incomparable(incomparable))
            | (Constraint::Incomparable(incomparable), Constraint::Range(range)) => {
                range.union_incomparable(db, incomparable)
            }

            (Constraint::NotEquivalent(left), Constraint::NotEquivalent(right)) => {
                left.union_not_equivalent(db, right)
            }

            (Constraint::NotEquivalent(not_equivalent), Constraint::Incomparable(incomparable))
            | (Constraint::Incomparable(incomparable), Constraint::NotEquivalent(not_equivalent)) => {
                not_equivalent.union_incomparable(db, incomparable)
            }

            (Constraint::Incomparable(left), Constraint::Incomparable(right)) => {
                left.union_incomparable(db, right)
            }
        }
    }

    fn negate_into(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        set: &mut ConstraintSet<'db>,
    ) {
        match self {
            Constraint::Range(constraint) => constraint.negate_into(db, typevar, set),
            Constraint::NotEquivalent(constraint) => constraint.negate_into(db, typevar, set),
            Constraint::Incomparable(constraint) => constraint.negate_into(db, typevar, set),
        }
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayConstraint<'a, 'db, D> {
            constraint: &'a Constraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.constraint {
                    Constraint::Range(constraint) => {
                        constraint.display(self.db, &self.typevar).fmt(f)
                    }
                    Constraint::NotEquivalent(constraint) => {
                        constraint.display(self.db, &self.typevar).fmt(f)
                    }
                    Constraint::Incomparable(constraint) => {
                        constraint.display(self.db, &self.typevar).fmt(f)
                    }
                }
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
        if lower.is_never() && upper.is_object() {
            return Satisfiable::Always;
        }

        Satisfiable::Constrained(Constraint::Range(RangeConstraint { lower, upper }))
    }
}

impl<'db> RangeConstraint<'db> {
    fn intersect_range(
        &self,
        db: &'db dyn Db,
        other: &RangeConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α (t₁ ∩ t₂)
        Simplifiable::from_one(Constraint::range(
            db,
            UnionType::from_elements(db, [self.lower, other.lower]),
            IntersectionType::from_elements(db, [self.upper, other.upper]),
        ))
    }

    fn intersect_not_equivalent(
        &self,
        db: &'db dyn Db,
        other: &NotEquivalentConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // If the range constraint says that the typevar must be equivalent to some type, and the
        // not-equivalent type says that it must not, we have a contradiction.
        if self.lower.is_equivalent_to(db, self.upper) && self.lower.is_equivalent_to(db, other.ty)
        {
            return Simplifiable::NeverSatisfiable;
        }

        // If the "hole" of the not-equivalent type is not contained in the range, the the
        // intersection simplifies to the range.
        if !self.lower.is_subtype_of(db, other.ty) || !other.ty.is_subtype_of(db, self.upper) {
            return Simplifiable::Simplified(Constraint::Range(self.clone()));
        }

        // Otherwise the result cannot be simplified.
        Simplifiable::NotSimplified(
            Constraint::Range(self.clone()),
            Constraint::NotEquivalent(other.clone()),
        )
    }

    fn intersect_incomparable(
        &self,
        db: &'db dyn Db,
        other: &IncomparableConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        if other.ty.is_subtype_of(db, self.lower) || self.upper.is_subtype_of(db, other.ty) {
            return Simplifiable::NeverSatisfiable;
        }

        // A range constraint and an incomparable constraint cannot be simplified.
        Simplifiable::NotSimplified(
            Constraint::Range(self.clone()),
            Constraint::Incomparable(other.clone()),
        )
    }

    fn union_range(
        &self,
        db: &'db dyn Db,
        other: &RangeConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // When one of the bounds is entirely contained within the other, the union simplifies to
        // the larger bounds.
        if self.lower.is_subtype_of(db, other.lower) && other.upper.is_subtype_of(db, self.upper) {
            return Simplifiable::Simplified(Constraint::Range(self.clone()));
        }
        if other.lower.is_subtype_of(db, self.lower) && self.upper.is_subtype_of(db, other.upper) {
            return Simplifiable::Simplified(Constraint::Range(other.clone()));
        }

        // Otherwise the result cannot be simplified.
        Simplifiable::NotSimplified(
            Constraint::Range(self.clone()),
            Constraint::Range(other.clone()),
        )
    }

    fn union_not_equivalent(
        &self,
        db: &'db dyn Db,
        other: &NotEquivalentConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // When the range constraint contains the "hole" from the non-equivalent constraint, then
        // the range constraint fills in the hole, and the result is always satisfiable.
        if self.lower.is_subtype_of(db, other.ty) && other.ty.is_subtype_of(db, self.upper) {
            return Simplifiable::AlwaysSatisfiable;
        }

        // Otherwise the range constraint is subsumed by the non-equivalent constraint.
        Simplifiable::Simplified(Constraint::NotEquivalent(other.clone()))
    }

    fn union_incomparable(
        &self,
        db: &'db dyn Db,
        other: &IncomparableConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // When the "pivot" of the incomparable constraint is not comparable with either bound of
        // the range constraint, the incomparable constraint subsumes the range constraint.
        if incomparable(db, self.lower, other.ty) && incomparable(db, self.upper, other.ty) {
            return Simplifiable::Simplified(Constraint::Incomparable(other.clone()));
        }

        // Otherwise the result cannot be simplified.
        Simplifiable::NotSimplified(
            Constraint::Range(self.clone()),
            Constraint::Incomparable(other.clone()),
        )
    }

    fn negate_into(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        set: &mut ConstraintSet<'db>,
    ) {
        // Lower bound:
        // ¬(s ≤ α) = ((α ≤ s) ∧ α ≠ s) ∨ (a ≁ s)
        set.union_clause(
            db,
            ConstraintClause::from_constraints(
                db,
                [
                    Constraint::range(db, Type::Never, self.lower).constrain(typevar),
                    Constraint::not_equivalent(db, self.lower).constrain(typevar),
                ],
            ),
        );
        set.union_constraint(
            db,
            Constraint::incomparable(db, self.lower).constrain(typevar),
        );

        // Upper bound:
        // ¬(α ≤ t) = ((t ≤ α) ∧ α ≠ t) ∨ (a ≁ t)
        set.union_clause(
            db,
            ConstraintClause::from_constraints(
                db,
                [
                    Constraint::range(db, self.upper, Type::object()).constrain(typevar),
                    Constraint::not_equivalent(db, self.upper).constrain(typevar),
                ],
            ),
        );
        set.union_constraint(
            db,
            Constraint::incomparable(db, self.upper).constrain(typevar),
        );
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayRangeConstraint<'a, 'db, D> {
            constraint: &'a RangeConstraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayRangeConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
pub(crate) struct NotEquivalentConstraint<'db> {
    ty: Type<'db>,
}

impl<'db> Constraint<'db> {
    /// Returns a new not-equivalent constraint.
    ///
    /// Panics if `ty` is not fully static.
    fn not_equivalent(db: &'db dyn Db, ty: Type<'db>) -> Satisfiable<Constraint<'db>> {
        debug_assert_eq!(ty, ty.top_materialization(db));
        Satisfiable::Constrained(Constraint::NotEquivalent(NotEquivalentConstraint { ty }))
    }
}

impl<'db> NotEquivalentConstraint<'db> {
    fn intersect_not_equivalent(
        &self,
        db: &'db dyn Db,
        other: &NotEquivalentConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        if self.ty.is_equivalent_to(db, other.ty) {
            return Simplifiable::Simplified(Constraint::NotEquivalent(self.clone()));
        }
        Simplifiable::NotSimplified(
            Constraint::NotEquivalent(self.clone()),
            Constraint::NotEquivalent(other.clone()),
        )
    }

    fn intersect_incomparable(
        &self,
        db: &'db dyn Db,
        other: &IncomparableConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // If the hole and pivot are comparable, then removing the hole from the incomparable
        // constraint doesn't do anything.
        if comparable(db, self.ty, other.ty) {
            return Simplifiable::Simplified(Constraint::Incomparable(other.clone()));
        }
        Simplifiable::NotSimplified(
            Constraint::NotEquivalent(self.clone()),
            Constraint::Incomparable(other.clone()),
        )
    }

    fn union_not_equivalent(
        &self,
        db: &'db dyn Db,
        other: &NotEquivalentConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        if self.ty.is_equivalent_to(db, other.ty) {
            return Simplifiable::Simplified(Constraint::NotEquivalent(self.clone()));
        }
        Simplifiable::AlwaysSatisfiable
    }

    fn union_incomparable(
        &self,
        db: &'db dyn Db,
        other: &IncomparableConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        // When the "hole" of the non-equivalent constraint and the "pivot" of the incomparable
        // constraint are not comparable, then the hole is covered by the incomparable constraint,
        // and the union is therefore always satisfied.
        if incomparable(db, self.ty, other.ty) {
            return Simplifiable::AlwaysSatisfiable;
        }

        // Otherwise, the hole and pivot are comparable, and the non-equivalent constraint subsumes
        // the incomparable constraint.
        Simplifiable::Simplified(Constraint::NotEquivalent(self.clone()))
    }

    fn negate_into(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        set: &mut ConstraintSet<'db>,
    ) {
        // Not equivalent:
        // ¬(α ≠ t) = (t ≤ α ≤ t)
        set.union_constraint(
            db,
            Constraint::range(db, self.ty, self.ty).constrain(typevar),
        );
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayNotEquivalentConstraint<'a, 'db, D> {
            constraint: &'a NotEquivalentConstraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayNotEquivalentConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "({} ≠ {})",
                    self.typevar,
                    self.constraint.ty.display(self.db)
                )
            }
        }

        DisplayNotEquivalentConstraint {
            constraint: self,
            typevar,
            db,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct IncomparableConstraint<'db> {
    ty: Type<'db>,
}

impl<'db> Constraint<'db> {
    /// Returns a new incomparable constraint.
    ///
    /// Panics if `ty` is not fully static.
    fn incomparable(db: &'db dyn Db, ty: Type<'db>) -> Satisfiable<Constraint<'db>> {
        debug_assert_eq!(ty, ty.top_materialization(db));

        // Every type is comparable to Never and to object.
        if ty.is_never() || ty.is_object() {
            return Satisfiable::Never;
        }

        Satisfiable::Constrained(Constraint::Incomparable(IncomparableConstraint { ty }))
    }
}

impl<'db> IncomparableConstraint<'db> {
    fn intersect_incomparable(
        &self,
        db: &'db dyn Db,
        other: &IncomparableConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        if self.ty.is_equivalent_to(db, other.ty) {
            return Simplifiable::Simplified(Constraint::Incomparable(other.clone()));
        }
        Simplifiable::NotSimplified(
            Constraint::Incomparable(self.clone()),
            Constraint::Incomparable(other.clone()),
        )
    }

    fn union_incomparable(
        &self,
        db: &'db dyn Db,
        other: &IncomparableConstraint<'db>,
    ) -> Simplifiable<Constraint<'db>> {
        if self.ty.is_equivalent_to(db, other.ty) {
            return Simplifiable::Simplified(Constraint::Incomparable(other.clone()));
        }
        Simplifiable::NotSimplified(
            Constraint::Incomparable(self.clone()),
            Constraint::Incomparable(other.clone()),
        )
    }

    fn negate_into(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        set: &mut ConstraintSet<'db>,
    ) {
        // Not comparable:
        // ¬(α ≁ t) = (t ≤ α) ∨ (α ≤ t)
        set.union_constraint(
            db,
            Constraint::range(db, Type::Never, self.ty).constrain(typevar),
        );
        set.union_constraint(
            db,
            Constraint::range(db, self.ty, Type::object()).constrain(typevar),
        );
    }

    fn display(&self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        struct DisplayIncomparableConstraint<'a, 'db, D> {
            constraint: &'a IncomparableConstraint<'db>,
            typevar: D,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayIncomparableConstraint<'_, '_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "({} ≁ {})",
                    self.typevar,
                    self.constraint.ty.display(self.db)
                )
            }
        }

        DisplayIncomparableConstraint {
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

    fn map<U>(self, mut f: impl FnMut(T) -> U) -> Simplifiable<U> {
        match self {
            Simplifiable::NeverSatisfiable => Simplifiable::NeverSatisfiable,
            Simplifiable::AlwaysSatisfiable => Simplifiable::AlwaysSatisfiable,
            Simplifiable::Simplified(t) => Simplifiable::Simplified(f(t)),
            Simplifiable::NotSimplified(t1, t2) => Simplifiable::NotSimplified(f(t1), f(t2)),
        }
    }
}
