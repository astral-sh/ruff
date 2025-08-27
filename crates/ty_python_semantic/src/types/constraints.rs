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
//! An individual constraint restricts the specialization of a single typevar to be within a
//! particular lower and upper bound: the typevar can only specialize to types that are a supertype
//! of the lower bound, and a subtype of the upper bound. (Note that lower and upper bounds are
//! fully static; we take the bottom and top materializations of the bounds to remove any gradual
//! forms if needed.) Either bound can be "closed" (where the bound is a valid specialization), or
//! "open" (where it is not).
//!
//! You can then build up more complex constraint sets using union, intersection, and negation
//! operations. We use a distributed normal form (DNF) representation, just like we do for types: a
//! [constraint set][ConstraintSet] is the union of zero or more [clauses][ConstraintClause], each
//! of which is the intersection of zero or more [individual constraints][AtomicConstraint]. Note
//! that the constraint set that contains no clauses is never satisfiable (`⋃ {} = 0`); and the
//! constraint set that contains a single clause, which contains no constraints, is always
//! satisfiable (`⋃ {⋂ {}} = 1`).
//!
//! NOTE: This module is currently in a transitional state: we've added a [`Constraints`] trait,
//! and updated all of our type property implementations to work on any impl of that trait. We have
//! added the DNF [`ConstraintSet`] representation, and updated all of our property checks to build
//! up a constraint set and then check whether it is ever or always satisfiable, as appropriate. We
//! are not yet inferring specializations from those constraints, and we will likely remove the
//! [`Constraints`] trait once everything has stabilized.

use std::fmt::Display;

use itertools::{EitherOrBoth, Itertools};
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

    #[allow(dead_code)]
    fn display(&self, db: &'db dyn Db) -> impl Display;
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

/// A set of constraints under which a type property holds.
///
/// We use a DNF representation, so a set contains a list of zero or more
/// [clauses][ConstraintClause], each of which is an intersection of zero or more
/// [constraints][AtomicConstraint].
///
/// This is called a "set of constraint sets", and denoted _𝒮_, in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
///
/// ### Invariants
///
/// - The clauses are simplified as much as possible — there are no two clauses in the set that can
///   be simplified into a single clause.
#[derive(Clone, Debug)]
pub(crate) struct ConstraintSet<'db> {
    clauses: SmallVec<[ConstraintClause<'db>; 2]>,
}

impl<'db> ConstraintSet<'db> {
    /// Returns the constraint set that is never satisfiable.
    fn never() -> Self {
        Self {
            clauses: smallvec![],
        }
    }

    /// Returns a constraint set that contains a single clause.
    fn singleton(clause: ConstraintClause<'db>) -> Self {
        Self {
            clauses: smallvec![clause],
        }
    }

    /// Updates this set to be the union of itself and a constraint.
    fn union_constraint(
        &mut self,
        db: &'db dyn Db,
        constraint: Satisfiable<AtomicConstraint<'db>>,
    ) {
        match constraint {
            // ... ∪ 0 = ...
            Satisfiable::Never => {}
            // ... ∪ 1 = 1
            Satisfiable::Always => {
                self.clauses.clear();
                self.clauses.push(ConstraintClause::always());
            }
            // Otherwise wrap the constraint into a singleton clause and use the logic below to add
            // it.
            Satisfiable::Constrained(constraint) => {
                self.union_clause(db, ConstraintClause::singleton(constraint));
            }
        }
    }

    /// Updates this set to be the union of itself and a clause. To maintain the invariants of this
    /// type, we must simplify this clause against all existing clauses, if possible.
    fn union_clause(&mut self, db: &'db dyn Db, mut clause: ConstraintClause<'db>) {
        // Naively, we would just append the new clause to the set's list of clauses. But that
        // doesn't ensure that the clauses are simplified with respect to each other. So instead,
        // we iterate through the list of existing clauses, and try to simplify the new clause
        // against each one in turn. (We can assume that the existing clauses are already
        // simplified with respect to each other, since we can assume that the invariant holds upon
        // entry to this method.)
        let prev = std::mem::take(&mut self.clauses);
        let mut existing_clauses = prev.into_iter();
        while let Some(existing) = existing_clauses.next() {
            // Try to simplify the new clause against an existing clause.
            match existing.union_clause(db, clause) {
                Simplified::Never => {
                    // If two clauses cancel out to 0, that does NOT cause the entire set to become
                    // 0.  We need to keep whatever clauses have already been added to the result,
                    // and also need to copy over any later clauses that we hadn't processed yet.
                    self.clauses.extend(existing_clauses);
                    return;
                }

                Simplified::Always => {
                    // If two clauses cancel out to 1, that makes the entire set 1, and all
                    // existing clauses are simplified away.
                    self.clauses.clear();
                    self.clauses.push(ConstraintClause::always());
                    return;
                }

                Simplified::Two(existing, c) => {
                    // We couldn't simplify the new clause relative to this existing clause, so add
                    // the existing clause to the result. Continue trying to simplify the new
                    // clause against the later existing clauses.
                    self.clauses.push(existing);
                    clause = c;
                }

                Simplified::One(c) => {
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
            self.union_clause(db, clause);
        }
    }

    /// Updates this set to be the intersection of itself and another set.
    fn intersect_set(&mut self, db: &'db dyn Db, other: &Self) {
        // This is the distributive law:
        // (A ∪ B) ∩ (C ∪ D ∪ E) = (A ∩ C) ∪ (A ∩ D) ∪ (A ∩ E) ∪ (B ∩ C) ∪ (B ∩ D) ∪ (B ∩ E)
        let self_clauses = std::mem::take(&mut self.clauses);
        for self_clause in &self_clauses {
            for other_clause in &other.clauses {
                match self_clause.intersect_clause(db, other_clause) {
                    Satisfiable::Never => continue,
                    Satisfiable::Always => {
                        self.clauses.clear();
                        self.clauses.push(ConstraintClause::always());
                        return;
                    }
                    Satisfiable::Constrained(clause) => self.union_clause(db, clause),
                }
            }
        }
    }

    #[allow(dead_code)]
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

    fn display(&self, db: &'db dyn Db) -> impl Display {
        self.display(db)
    }
}

/// The intersection of zero or more atomic constraints.
///
/// This is called a "constraint set", and denoted _C_, in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
///
/// ### Invariants
///
/// - No two constraints in the clause will constrain the same typevar.
/// - The constraints are sorted by typevar.
#[derive(Clone, Debug)]
pub(crate) struct ConstraintClause<'db> {
    constraints: SmallVec<[AtomicConstraint<'db>; 1]>,
}

impl<'db> ConstraintClause<'db> {
    /// Returns the clause that is always satisfiable.
    fn always() -> Self {
        Self {
            constraints: smallvec![],
        }
    }

    /// Returns a clause containing a single constraint.
    fn singleton(constraint: AtomicConstraint<'db>) -> Self {
        Self {
            constraints: smallvec![constraint],
        }
    }

    /// Returns whether this constraint is always satisfiable.
    fn is_always(&self) -> bool {
        self.constraints.is_empty()
    }

    /// Returns the intersection of this clause and an atomic constraint.
    fn intersect_constraint(
        &mut self,
        db: &'db dyn Db,
        constraint: AtomicConstraint<'db>,
    ) -> Satisfiable<()> {
        // If the clause does not already contain a constraint for this typevar, we just insert the
        // new constraint into the clause and return.
        let index = match (self.constraints)
            .binary_search_by_key(&constraint.typevar, |existing| existing.typevar)
        {
            Ok(index) => index,
            Err(index) => {
                self.constraints.insert(index, constraint);
                return Satisfiable::Constrained(());
            }
        };

        // If the clause already contains a constraint for this typevar, we need to intersect the
        // existing and new constraints, and simplify the clause accordingly.
        match self.constraints[index].intersect(db, constraint) {
            // ... ∩ 0 ∩ ... == 0
            // If the intersected constraint cannot be satisfied, that causes this whole clause to
            // be unsatisfiable too.
            Satisfiable::Never => Satisfiable::Never,

            // ... ∩ 1 ∩ ... == ...
            // If the intersected result is always satisfied, then the constraint no longer
            // contributes anything to the clause, and can be removed.
            Satisfiable::Always => {
                self.constraints.remove(index);
                if self.is_always() {
                    // If there are no further constraints in the clause, the clause is now always
                    // satisfied.
                    Satisfiable::Always
                } else {
                    Satisfiable::Constrained(())
                }
            }

            // ... ∩ X ∩ ... == ... ∩ X ∩ ...
            // If the intersection is a single constraint, we can reuse the existing constraint's
            // place in the clause's constraint list.
            Satisfiable::Constrained(constraint) => {
                self.constraints[index] = constraint;
                Satisfiable::Constrained(())
            }
        }
    }

    /// Returns the intersection of this clause with another.
    fn intersect_clause(&self, db: &'db dyn Db, other: &Self) -> Satisfiable<Self> {
        // Add each `other` constraint in turn. Short-circuit if the result ever becomes 0.
        let mut result = self.clone();
        for constraint in &other.constraints {
            match result.intersect_constraint(db, *constraint) {
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

    /// Returns the union of this clause and another.
    fn union_clause(self, db: &'db dyn Db, other: Self) -> Simplified<Self> {
        if self.is_always() || other.is_always() {
            return Simplified::One(Self::always());
        }
        if self.subsumes_via_intersection(db, &other) {
            return Simplified::One(other);
        }
        if other.subsumes_via_intersection(db, &self) {
            return Simplified::One(self);
        }
        if let Some(simplified) = self.simplifies_via_union(db, &other) {
            if simplified.is_always() {
                return Simplified::Always;
            }
            return Simplified::One(simplified);
        }
        Simplified::Two(self, other)
    }

    /// Returns whether this clause subsumes `other` via intersection — that is, if the
    /// intersection of `self` and `other` is `self`.
    fn subsumes_via_intersection(&self, db: &'db dyn Db, other: &Self) -> bool {
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
                    if !self_constraint.subsumes(db, *other_constraint) {
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
    fn simplifies_via_union(&self, db: &'db dyn Db, other: &Self) -> Option<Self> {
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
                    let union_constraint = match self_constraint.union(db, *other_constraint) {
                        Simplified::Two(_, _) => {
                            // The constraints for this typevar are not identical, nor do they
                            // simplify.
                            return None;
                        }
                        Simplified::One(union_constraint) => Some(union_constraint),
                        Simplified::Always => None,
                        Simplified::Never => {
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

    /// Returns the negation of this clause.
    fn negate(&self, db: &'db dyn Db) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::never();
        for constraint in &self.constraints {
            result.union_set(db, constraint.negate(db));
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

/// A constraint on a single typevar, providing lower and upper bounds for the types that it can
/// specialize to. The lower and upper bounds can each be either _closed_ (the bound itself is
/// included) or _open_ (the bound itself is not included).
///
/// We render constraints using a normal interval notation: `[s, t]`, `(s, t)`, `[s, t)`, or
/// `(s, t]`, where a square bracket indicates that bound is closed, and a parenthesis indicates
/// that it is open.
///
/// We will also sometimes render them graphically: `s┠──┨t`, `s╟──╢t`, `s┠──╢t`, or `s╟──┨t`,
/// where a solid bar indicates a closed bound, and a double bar indicates an open  bound.
///
/// ### Invariants
///
/// - The bounds must actually constraint the typevar. If the typevar can be specialized to any
///   type, or if there is no valid type that it can be specialized to, then we don't create an
///   `AtomicConstraint` for the typevar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AtomicConstraint<'db> {
    typevar: BoundTypeVarInstance<'db>,
    lower: ConstraintBound<'db>,
    upper: ConstraintBound<'db>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConstraintBound<'db> {
    Open(Type<'db>),
    Closed(Type<'db>),
}

impl<'db> ConstraintBound<'db> {
    fn flip(self) -> Self {
        match self {
            ConstraintBound::Open(bound) => ConstraintBound::Closed(bound),
            ConstraintBound::Closed(bound) => ConstraintBound::Open(bound),
        }
    }

    fn bound_type(self) -> Type<'db> {
        match self {
            ConstraintBound::Open(bound) => bound,
            ConstraintBound::Closed(bound) => bound,
        }
    }

    fn is_open(self) -> bool {
        matches!(self, ConstraintBound::Open(_))
    }

    /// Returns the minimum of two upper bounds.
    ///
    /// We use intersection to combine the types of the bounds (mnemonic: minimum and intersection
    /// both make the result smaller).
    ///
    /// If either of the input upper bounds is open — `[s, t)` or `(s, t)` — then `t` must not be
    /// included in the result. If the intersection is equivalent to `t`, then the result must
    /// therefore be an open bound. If the intersection is not equivalent to `t`, then it must be
    /// smaller than `t`, since intersection cannot make the result larger; therefore `t` is
    /// already not included in the result, and the bound will be closed.
    fn min_upper(self, db: &'db dyn Db, other: Self) -> Self {
        let result_bound =
            IntersectionType::from_elements(db, [self.bound_type(), other.bound_type()]);
        match (self, other) {
            (ConstraintBound::Open(bound), _) | (_, ConstraintBound::Open(bound))
                if bound.is_equivalent_to(db, result_bound) =>
            {
                ConstraintBound::Open(result_bound)
            }
            _ => ConstraintBound::Closed(result_bound),
        }
    }

    /// Returns the maximum of two upper bounds.
    ///
    /// We use union to combine the types of the bounds (mnemonic: maximum and union both make the
    /// result larger).
    ///
    /// For the result to be open, the union must be equivalent to one of the input bounds. (Union
    /// can only make types "bigger", so if the union is not equivalent to either input, it is
    /// strictly larger than both, and the result bound should therefore be closed.)
    ///
    /// There are only three situations where the result is open:
    ///
    /// ```text
    /// ────╢t₁    ────╢t₁    ────╢t₁    ────┨t₁    ────┨t₁    ────┨t₁    ────╢t₁
    /// ──╢t₂      ──┨t₂      ────╢t₂    ──╢t₂      ──┨t₂      ────┨t₂    ────┨t₂
    ///     ⇓          ⇓          ⇓          ⇓          ⇓          ⇓          ⇓
    /// ────╢t₁    ────╢t₁    ────╢t₁    ────┨t₁    ────┨t₁    ────┨t₁    ────┨t₁
    /// ```
    ///
    /// In all of these cases, the union is equivalent to `t₁`. (There are symmetric cases
    /// where the intersection is equivalent to `t₂`, but we'll handle them by deciding to call the
    /// "smaller" input `t₁`.) Note that the result is open if `t₂ < t₁` (_strictly_ less than), or
    /// if _both_ inputs are open and `t₁ = t₂`. In all other cases, the result is closed.
    fn max_upper(self, db: &'db dyn Db, other: Self) -> Self {
        let result_bound = UnionType::from_elements(db, [self.bound_type(), other.bound_type()]);
        match (self, other) {
            (ConstraintBound::Open(self_bound), ConstraintBound::Open(other_bound))
                if self_bound.is_equivalent_to(db, result_bound)
                    || other_bound.is_equivalent_to(db, result_bound) =>
            {
                ConstraintBound::Open(result_bound)
            }

            (ConstraintBound::Closed(other_bound), ConstraintBound::Open(open_bound))
            | (ConstraintBound::Open(open_bound), ConstraintBound::Closed(other_bound))
                if open_bound.is_equivalent_to(db, result_bound)
                    && other_bound.is_subtype_of(db, open_bound)
                    && !other_bound.is_equivalent_to(db, open_bound) =>
            {
                ConstraintBound::Open(result_bound)
            }

            _ => ConstraintBound::Closed(result_bound),
        }
    }

    /// Returns the minimum of two lower bounds.
    ///
    /// We use intersection to combine the types of the bounds (mnemonic: minimum and intersection
    /// both make the result smaller).
    ///
    /// For the result to be open, the intersection must be equivalent to one of the input bounds.
    /// (Intersection can only make types "smaller", so if the intersection is not equivalent to
    /// either input, it is strictly smaller than both, and the result bound should therefore be
    /// closed.)
    ///
    /// There are only three situations where the result is open:
    ///
    /// ```text
    /// s₁╟────    s₁╟────    s₁╟────    s₁┠────    s₁┠────    s₁┠────    s₁╟────
    /// s₂╟──      s₂┠──      s₂╟────    s₂╟──      s₂┠──      s₂┠────    s₂┠────
    ///     ⇓          ⇓          ⇓          ⇓          ⇓          ⇓          ⇓
    /// s₁╟────    s₁╟────    s₁╟────    s₁┠────    s₁┠────    s₁┠────    s₁┠────
    /// ```
    ///
    /// In all of these cases, the intersection is equivalent to `s₁`. (There are symmetric cases
    /// where the intersection is equivalent to `s₂`, but we'll handle them by deciding to call the
    /// "smaller" input `s₁`.) Note that the result is open if `s₁ < s₂` (_strictly_ less than), or
    /// if _both_ inputs are open and `s₁ = s₂`. In all other cases, the result is closed.
    fn min_lower(self, db: &'db dyn Db, other: Self) -> Self {
        let result_bound =
            IntersectionType::from_elements(db, [self.bound_type(), other.bound_type()]);
        match (self, other) {
            (ConstraintBound::Open(self_bound), ConstraintBound::Open(other_bound))
                if self_bound.is_equivalent_to(db, result_bound)
                    || other_bound.is_equivalent_to(db, result_bound) =>
            {
                ConstraintBound::Open(result_bound)
            }

            (ConstraintBound::Closed(other_bound), ConstraintBound::Open(open_bound))
            | (ConstraintBound::Open(open_bound), ConstraintBound::Closed(other_bound))
                if open_bound.is_equivalent_to(db, result_bound)
                    && open_bound.is_subtype_of(db, other_bound)
                    && !open_bound.is_equivalent_to(db, other_bound) =>
            {
                ConstraintBound::Open(result_bound)
            }

            _ => ConstraintBound::Closed(result_bound),
        }
    }

    /// Returns the maximum of two lower bounds.
    ///
    /// We use union to combine the types of the bounds (mnemonic: maximum and union both make the
    /// result larger).
    ///
    /// If either of the input lower bounds is open — `(s, t]` or `(s, t)` — then `s` must not be
    /// included in the result. If the union is equivalent to `s`, then the result must therefore
    /// be an open bound. If the union is not equivalent to `s`, then it must be larger than `s`,
    /// since union cannot make the result smaller; therefore `s` is already not included in the
    /// result, and the bound will be closed.
    fn max_lower(self, db: &'db dyn Db, other: Self) -> Self {
        let result_bound = UnionType::from_elements(db, [self.bound_type(), other.bound_type()]);
        match (self, other) {
            (ConstraintBound::Open(bound), _) | (_, ConstraintBound::Open(bound))
                if bound.is_equivalent_to(db, result_bound) =>
            {
                ConstraintBound::Open(result_bound)
            }
            _ => ConstraintBound::Closed(result_bound),
        }
    }
}

impl<'db> AtomicConstraint<'db> {
    /// Returns a new atomic constraint, ensuring that all invariants are held.
    fn new(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        lower: ConstraintBound<'db>,
        upper: ConstraintBound<'db>,
    ) -> Satisfiable<Self> {
        let lower_type = lower.bound_type();
        let upper_type = upper.bound_type();
        if !lower_type.is_subtype_of(db, upper_type) {
            return Satisfiable::Never;
        }
        if (lower.is_open() || upper.is_open()) && lower_type.is_equivalent_to(db, upper_type) {
            return Satisfiable::Never;
        }
        if let (ConstraintBound::Closed(lower), ConstraintBound::Closed(upper)) = (lower, upper) {
            if lower.is_never() && upper.is_object(db) {
                return Satisfiable::Always;
            }
        }
        Satisfiable::Constrained(Self {
            typevar,
            lower,
            upper,
        })
    }

    /// Returns the negation of this atomic constraint.
    fn negate(self, db: &'db dyn Db) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::never();
        result.union_constraint(
            db,
            Self::new(
                db,
                self.typevar,
                ConstraintBound::Closed(Type::Never),
                self.lower.flip(),
            ),
        );
        result.union_constraint(
            db,
            Self::new(
                db,
                self.typevar,
                self.upper.flip(),
                ConstraintBound::Closed(Type::object(db)),
            ),
        );
        result
    }

    /// Returns whether `self` has tighter bounds than `other` — that is, if the intersection of
    /// `self` and `other` is `self`.
    fn subsumes(self, db: &'db dyn Db, other: Self) -> bool {
        debug_assert!(self.typevar == other.typevar);
        self.intersect(db, other) == Satisfiable::Constrained(self)
    }

    /// Returns the intersection of this atomic constraint and another.
    ///
    /// Panics if the two constraints have different typevars.
    fn intersect(self, db: &'db dyn Db, other: Self) -> Satisfiable<Self> {
        debug_assert!(self.typevar == other.typevar);

        // The result is always `max_lower(s₁,s₂) : min_upper(t₁,t₂)`. (Note that `max_lower` and
        // `min_upper` determine whether the corresponding bound is open or closed.)
        Self::new(
            db,
            self.typevar,
            self.lower.max_lower(db, other.lower),
            self.upper.min_upper(db, other.upper),
        )
    }

    /// Returns the union of this atomic constraint and another.
    ///
    /// Panics if the two constraints have different typevars.
    fn union(self, db: &'db dyn Db, other: Self) -> Simplified<Self> {
        debug_assert!(self.typevar == other.typevar);

        // When the two constraints are disjoint, then they cannot be simplified.
        //
        //   self:    s₁┠─┨t₁
        //   other:           s₂┠─┨t₂
        //
        //   result:  s₁┠─┨t₁ s₂┠─┨t₂
        let is_subtype_of = |left: ConstraintBound<'db>, right: ConstraintBound<'db>| {
            let left_type = left.bound_type();
            let right_type = right.bound_type();
            if !left_type.is_subtype_of(db, right_type) {
                return false;
            }
            if left.is_open() && right.is_open() {
                return !left_type.is_equivalent_to(db, right_type);
            }
            true
        };
        if !is_subtype_of(self.lower, other.upper) || !is_subtype_of(other.lower, self.upper) {
            return Simplified::Two(self, other);
        }

        // Otherwise the result is `min_lower(s₁,s₂) : max_upper(t₁,t₂)`. (Note that `min_lower`
        // and `max_upper` determine whether the corresponding bound is open or closed.)
        Simplified::from_one(Self::new(
            db,
            self.typevar,
            self.lower.min_lower(db, other.lower),
            self.upper.max_upper(db, other.upper),
        ))
    }

    fn display(self, db: &'db dyn Db) -> impl Display {
        struct DisplayAtomicConstraint<'db> {
            constraint: AtomicConstraint<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplayAtomicConstraint<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("(")?;
                match self.constraint.lower {
                    ConstraintBound::Closed(bound) if bound.is_never() => {}
                    ConstraintBound::Closed(bound) => write!(f, "{} ≤ ", bound.display(self.db))?,
                    ConstraintBound::Open(bound) => write!(f, "{} < ", bound.display(self.db))?,
                }
                self.constraint.typevar.display(self.db).fmt(f)?;
                match self.constraint.upper {
                    ConstraintBound::Closed(bound) if bound.is_object(self.db) => {}
                    ConstraintBound::Closed(bound) => write!(f, " ≤ {}", bound.display(self.db))?,
                    ConstraintBound::Open(bound) => write!(f, " < {}", bound.display(self.db))?,
                }
                f.write_str(")")
            }
        }

        DisplayAtomicConstraint {
            constraint: self,
            db,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Satisfiable<T> {
    Never,
    Always,
    Constrained(T),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Simplified<T> {
    Never,
    Always,
    One(T),
    Two(T, T),
}

impl<T> Simplified<T> {
    fn from_one(constraint: Satisfiable<T>) -> Self {
        match constraint {
            Satisfiable::Never => Simplified::Never,
            Satisfiable::Always => Simplified::Always,
            Satisfiable::Constrained(constraint) => Simplified::One(constraint),
        }
    }
}
