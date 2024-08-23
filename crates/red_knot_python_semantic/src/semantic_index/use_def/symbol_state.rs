//! Track visible definitions of a symbol, and applicable constraints per definition.
//!
//! These data structures operate entirely on scope-local newtype-indices for definitions and
//! constraints, referring to their location in the `all_definitions` and `all_constraints`
//! indexvecs in [`super::UseDefMapBuilder`].
//!
//! We need to track arbitrary associations between definitions and constraints, not just a single
//! set of currently dominating constraints (where "dominating" means "control flow must have
//! passed through it to reach this point"), because we can have dominating constraints that apply
//! to some definitions but not others, as in this code:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     if flag2:
//!         x = 2 if flag else None
//!     x
//! ```
//!
//! The `x is not None` constraint dominates the final use of `x`, but it applies only to the first
//! definition of `x`, not the second, so `None` is a possible value for `x`.
//!
//! And we can't just track, for each definition, an index into a list of dominating constraints,
//! either, because we can have definitions which are still visible, but subject to constraints
//! that are no longer dominating, as in this code:
//!
//! ```python
//! x = 0
//! if flag1:
//!     x = 1 if flag2 else None
//!     assert x is not None
//! x
//! ```
//!
//! From the point of view of the final use of `x`, the `x is not None` constraint no longer
//! dominates, but it does dominate the `x = 1 if flag2 else None` definition, so we have to keep
//! track of that.
//!
//! The data structures used here ([`BitSet`] and [`smallvec::SmallVec`]) optimize for keeping all
//! data inline (avoiding lots of scattered allocations) in small-to-medium cases, and falling back
//! to heap allocation to be able to scale to arbitrary numbers of definitions and constraints when
//! needed.
use super::bitset::{BitSet, BitSetIterator};
use ruff_index::newtype_index;
use smallvec::SmallVec;

/// A newtype-index for a definition in a particular scope.
#[newtype_index]
pub(super) struct ScopedDefinitionId;

/// A newtype-index for a constraint expression in a particular scope.
#[newtype_index]
pub(super) struct ScopedConstraintId;

/// Can reference this * 64 total definitions inline; more will fall back to the heap.
const INLINE_DEFINITION_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing visible definitions of a symbol in a scope.
type Definitions = BitSet<INLINE_DEFINITION_BLOCKS>;
type DefinitionsIterator<'a> = BitSetIterator<'a, INLINE_DEFINITION_BLOCKS>;

/// Can reference this * 64 total constraints inline; more will fall back to the heap.
const INLINE_CONSTRAINT_BLOCKS: usize = 2;

/// Can keep inline this many visible definitions per symbol at a given time; more will go to heap.
const INLINE_VISIBLE_DEFINITIONS_PER_SYMBOL: usize = 4;

/// One [`BitSet`] of applicable [`ScopedConstraintId`] per visible definition.
type InlineConstraintArray =
    [BitSet<INLINE_CONSTRAINT_BLOCKS>; INLINE_VISIBLE_DEFINITIONS_PER_SYMBOL];
type Constraints = SmallVec<InlineConstraintArray>;
type ConstraintsIterator<'a> = std::slice::Iter<'a, BitSet<INLINE_CONSTRAINT_BLOCKS>>;
type ConstraintsIntoIterator = smallvec::IntoIter<InlineConstraintArray>;

/// Visible definitions and narrowing constraints for a single symbol at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolState {
    /// [`BitSet`]: which [`ScopedDefinitionId`] are visible for this symbol?
    visible_definitions: Definitions,

    /// For each definition, which [`ScopedConstraintId`] apply?
    ///
    /// This is a [`smallvec::SmallVec`] which should always have one [`BitSet`] of constraints per
    /// definition in `visible_definitions`.
    constraints: Constraints,

    /// Could the symbol be unbound at this point?
    may_be_unbound: bool,
}

/// A single [`ScopedDefinitionId`] with an iterator of its applicable [`ScopedConstraintId`].
#[derive(Debug)]
pub(super) struct DefinitionIdWithConstraints<'a> {
    pub(super) definition: ScopedDefinitionId,
    pub(super) constraint_ids: ConstraintIdIterator<'a>,
}

impl SymbolState {
    /// Return a new [`SymbolState`] representing an unbound symbol.
    pub(super) fn unbound() -> Self {
        Self {
            visible_definitions: Definitions::default(),
            constraints: Constraints::default(),
            may_be_unbound: true,
        }
    }

    /// Return a new [`SymbolState`] representing a symbol with a single visible definition.
    pub(super) fn with(definition_id: ScopedDefinitionId) -> Self {
        let mut constraints = Constraints::with_capacity(1);
        constraints.push(BitSet::default());
        Self {
            visible_definitions: Definitions::with(definition_id.into()),
            constraints,
            may_be_unbound: false,
        }
    }

    /// Add Unbound as a possibility for this symbol.
    pub(super) fn add_unbound(&mut self) {
        self.may_be_unbound = true;
    }

    /// Add given constraint to all currently-visible definitions.
    pub(super) fn add_constraint(&mut self, constraint_id: ScopedConstraintId) {
        for bitset in &mut self.constraints {
            bitset.insert(constraint_id.into());
        }
    }

    /// Merge another [`SymbolState`] into this one.
    pub(super) fn merge(&mut self, b: SymbolState) {
        let mut a = Self {
            visible_definitions: Definitions::default(),
            constraints: Constraints::default(),
            may_be_unbound: self.may_be_unbound || b.may_be_unbound,
        };
        std::mem::swap(&mut a, self);
        let mut a_defs_iter = a.visible_definitions.iter();
        let mut b_defs_iter = b.visible_definitions.iter();
        let mut a_constraints_iter = a.constraints.into_iter();
        let mut b_constraints_iter = b.constraints.into_iter();

        let mut opt_a_def: Option<u32> = a_defs_iter.next();
        let mut opt_b_def: Option<u32> = b_defs_iter.next();

        // Iterate through the definitions from `a` and `b`, always processing the lower definition
        // ID first, and pushing each definition onto the merged `SymbolState` with its
        // constraints. If a definition is found in both `a` and `b`, push it with the intersection
        // of the constraints from the two paths; a constraint that applies from only one possible
        // path is irrelevant.

        // Helper to push `def`, with constraints in `constraints_iter`, onto `self`.
        let push = |def, constraints_iter: &mut ConstraintsIntoIterator, merged: &mut Self| {
            merged.visible_definitions.insert(def);
            // SAFETY: we only ever create SymbolState with either no definitions and no constraint
            // bitsets (`::unbound`) or one definition and one constraint bitset (`::with`), and
            // `::merge` always pushes one definition and one constraint bitset together (just
            // below), so the number of definitions and the number of constraint bitsets can never
            // get out of sync.
            let constraints = constraints_iter
                .next()
                .expect("definitions and constraints length mismatch");
            merged.constraints.push(constraints);
        };

        loop {
            match (opt_a_def, opt_b_def) {
                (Some(a_def), Some(b_def)) => match a_def.cmp(&b_def) {
                    std::cmp::Ordering::Less => {
                        // Next definition ID is only in `a`, push it to `self` and advance `a`.
                        push(a_def, &mut a_constraints_iter, self);
                        opt_a_def = a_defs_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        // Next definition ID is only in `b`, push it to `self` and advance `b`.
                        push(b_def, &mut b_constraints_iter, self);
                        opt_b_def = b_defs_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        // Next definition is in both; push to `self` and intersect constraints.
                        push(a_def, &mut b_constraints_iter, self);
                        // SAFETY: we only ever create SymbolState with either no definitions and
                        // no constraint bitsets (`::unbound`) or one definition and one constraint
                        // bitset (`::with`), and `::merge` always pushes one definition and one
                        // constraint bitset together (just below), so the number of definitions
                        // and the number of constraint bitsets can never get out of sync.
                        let a_constraints = a_constraints_iter
                            .next()
                            .expect("definitions and constraints length mismatch");
                        // If the same definition is visible through both paths, any constraint
                        // that applies on only one path is irrelevant to the resulting type from
                        // unioning the two paths, so we intersect the constraints.
                        self.constraints
                            .last_mut()
                            .unwrap()
                            .intersect(&a_constraints);
                        opt_a_def = a_defs_iter.next();
                        opt_b_def = b_defs_iter.next();
                    }
                },
                (Some(a_def), None) => {
                    // We've exhausted `b`, just push the def from `a` and move on to the next.
                    push(a_def, &mut a_constraints_iter, self);
                    opt_a_def = a_defs_iter.next();
                }
                (None, Some(b_def)) => {
                    // We've exhausted `a`, just push the def from `b` and move on to the next.
                    push(b_def, &mut b_constraints_iter, self);
                    opt_b_def = b_defs_iter.next();
                }
                (None, None) => break,
            }
        }
    }

    /// Get iterator over visible definitions with constraints.
    pub(super) fn visible_definitions(&self) -> DefinitionIdWithConstraintsIterator {
        DefinitionIdWithConstraintsIterator {
            definitions: self.visible_definitions.iter(),
            constraints: self.constraints.iter(),
        }
    }

    /// Could the symbol be unbound?
    pub(super) fn may_be_unbound(&self) -> bool {
        self.may_be_unbound
    }
}

/// The default state of a symbol (if we've seen no definitions of it) is unbound.
impl Default for SymbolState {
    fn default() -> Self {
        SymbolState::unbound()
    }
}

#[derive(Debug)]
pub(super) struct DefinitionIdWithConstraintsIterator<'a> {
    definitions: DefinitionsIterator<'a>,
    constraints: ConstraintsIterator<'a>,
}

impl<'a> Iterator for DefinitionIdWithConstraintsIterator<'a> {
    type Item = DefinitionIdWithConstraints<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.definitions.next(), self.constraints.next()) {
            (None, None) => None,
            (Some(def), Some(constraints)) => Some(DefinitionIdWithConstraints {
                definition: ScopedDefinitionId::from_u32(def),
                constraint_ids: ConstraintIdIterator {
                    wrapped: constraints.iter(),
                },
            }),
            // SAFETY: see above.
            _ => unreachable!("definitions and constraints length mismatch"),
        }
    }
}

impl std::iter::FusedIterator for DefinitionIdWithConstraintsIterator<'_> {}

#[derive(Debug)]
pub(super) struct ConstraintIdIterator<'a> {
    wrapped: BitSetIterator<'a, INLINE_CONSTRAINT_BLOCKS>,
}

impl Iterator for ConstraintIdIterator<'_> {
    type Item = ScopedConstraintId;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped.next().map(ScopedConstraintId::from_u32)
    }
}

impl std::iter::FusedIterator for ConstraintIdIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::{ScopedConstraintId, ScopedDefinitionId, SymbolState};

    impl SymbolState {
        pub(crate) fn assert(&self, may_be_unbound: bool, expected: &[&str]) {
            assert_eq!(self.may_be_unbound(), may_be_unbound);
            let actual = self
                .visible_definitions()
                .map(|def_id_with_constraints| {
                    format!(
                        "{}<{}>",
                        def_id_with_constraints.definition.as_u32(),
                        def_id_with_constraints
                            .constraint_ids
                            .map(ScopedConstraintId::as_u32)
                            .map(|idx| idx.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn unbound() {
        let cd = SymbolState::unbound();

        cd.assert(true, &[]);
    }

    #[test]
    fn with() {
        let cd = SymbolState::with(ScopedDefinitionId::from_u32(0));

        cd.assert(false, &["0<>"]);
    }

    #[test]
    fn add_unbound() {
        let mut cd = SymbolState::with(ScopedDefinitionId::from_u32(0));
        cd.add_unbound();

        cd.assert(true, &["0<>"]);
    }

    #[test]
    fn add_constraint() {
        let mut cd = SymbolState::with(ScopedDefinitionId::from_u32(0));
        cd.add_constraint(ScopedConstraintId::from_u32(0));

        cd.assert(false, &["0<0>"]);
    }

    #[test]
    fn merge() {
        // merging the same definition with the same constraint keeps the constraint
        let mut cd0a = SymbolState::with(ScopedDefinitionId::from_u32(0));
        cd0a.add_constraint(ScopedConstraintId::from_u32(0));

        let mut cd0b = SymbolState::with(ScopedDefinitionId::from_u32(0));
        cd0b.add_constraint(ScopedConstraintId::from_u32(0));

        cd0a.merge(cd0b);
        let mut cd0 = cd0a;
        cd0.assert(false, &["0<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut cd1a = SymbolState::with(ScopedDefinitionId::from_u32(1));
        cd1a.add_constraint(ScopedConstraintId::from_u32(1));

        let mut cd1b = SymbolState::with(ScopedDefinitionId::from_u32(1));
        cd1b.add_constraint(ScopedConstraintId::from_u32(2));

        cd1a.merge(cd1b);
        let cd1 = cd1a;
        cd1.assert(false, &["1<>"]);

        // merging a constrained definition with unbound keeps both
        let mut cd2a = SymbolState::with(ScopedDefinitionId::from_u32(2));
        cd2a.add_constraint(ScopedConstraintId::from_u32(3));

        let cd2b = SymbolState::unbound();

        cd2a.merge(cd2b);
        let cd2 = cd2a;
        cd2.assert(true, &["2<3>"]);

        // merging different definitions keeps them each with their existing constraints
        cd0.merge(cd2);
        let cd = cd0;
        cd.assert(true, &["0<0>", "2<3>"]);
    }
}
