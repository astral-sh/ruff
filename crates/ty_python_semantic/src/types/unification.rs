//! Implements the _tallying_ algorithm from [[POPL2015][]], which finds the unification of a
//! set of subtyping constraints.
//!
//! [POPL2015]: https://doi.org/10.1145/2676726.2676991

// XXX
#![allow(dead_code)]

use crate::Db;
use crate::types::{IntersectionType, Type, TypeVarInstance, UnionType};

/// A constraint that the type `s` must be a subtype of the type `t`. Tallying will find all
/// substitutions of any type variables in `s` and `t` that ensure that this subtyping holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Constraint<'db> {
    pub(crate) lower: Type<'db>,
    pub(crate) typevar: TypeVarInstance<'db>,
    pub(crate) upper: Type<'db>,
}

impl<'db> Constraint<'db> {
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
}

/// A set of merged constraints. We guarantee that no constraint in the set subsumes another, and
/// that no two constraints in the set have the same typevar.
///
/// This is denoted _C_ in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConstraintSet<'db> {
    constraints: Vec<Constraint<'db>>,
}

impl<'db> ConstraintSet<'db> {
    /// Returns an empty constraint set
    fn empty() -> Self {
        Self {
            constraints: vec![],
        }
    }

    /// Adds a new constraint to this set, ensuring that no constraint in the set subsumes another,
    /// and that no two constraints in the set have the same typevar.
    fn add(&mut self, db: &'db dyn Db, constraint: Constraint<'db>) {
        for existing in &mut self.constraints {
            if constraint.typevar == existing.typevar {
                existing.merge(db, constraint);
                return;
            }
        }
        self.constraints.push(constraint);
    }

    /// Combines two constraint sets, merging any constraints that share the same typevar.
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
}

/// A set of constraint sets.
///
/// This is denoted _𝒮_ in [[POPL2015][]].
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConstraintSetSet<'db> {
    sets: Vec<ConstraintSet<'db>>,
}

impl<'db> ConstraintSetSet<'db> {
    /// Returns the set of constraint sets that is unsolvable.
    pub(crate) fn none() -> Self {
        Self { sets: vec![] }
    }

    /// Returns the set of constraints set that is always satisfied.
    pub(crate) fn always() -> Self {
        Self {
            sets: vec![ConstraintSet::empty()],
        }
    }

    /// Adds a new constraint set to this set, ensuring that no constraint set in the set subsumes
    /// another.
    fn add(&mut self, db: &'db dyn Db, constraint_set: ConstraintSet<'db>) {
        for existing in &mut self.sets {
            // If there is an existing constraint set that subsumes (or is subsumed by) the new
            // one, we want to keep the _subsumed_ constraint set.
            if constraint_set.subsumes(db, existing) {
                return;
            } else if existing.subsumes(db, &constraint_set) {
                *existing = constraint_set;
                return;
            }
        }
        self.sets.push(constraint_set);
    }

    /// Intersects two sets of constraint sets.
    ///
    /// This is the ⊓ operator from [[POPL15][]], Definition 3.5.
    ///
    /// [POPL2015]: https://doi.org/10.1145/2676726.2676991
    fn intersect(&self, db: &'db dyn Db, other: &Self) -> Self {
        let mut result = Self::none();
        for self_set in &self.sets {
            for other_set in &other.sets {
                let mut new_set = self_set.clone();
                new_set.combine(db, other_set);
                result.add(db, new_set);
            }
        }
        result
    }

    /// union two sets of constraint sets.
    ///
    /// This is the ⊔ operator from [[POPL15][]], Definition 3.5.
    ///
    /// [POPL2015]: https://doi.org/10.1145/2676726.2676991
    fn union(&mut self, db: &'db dyn Db, other: Self) {
        for set in other.sets {
            self.add(db, set);
        }
    }
}
