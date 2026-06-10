//! # Narrowing constraints
//!
//! When building a semantic index for a file, we associate each binding with a narrowing
//! constraint, which constrains the type of the binding's place. A binding can be associated with
//! a different narrowing constraint at different points in a file. See the `use_def` module for
//! more details.
//!
//! A narrowing constraint is a boolean formula over predicates such as `isinstance(x, A)`.
//! Internally, we store these formulas in a ternary decision diagram (TDD). Each interior node has
//! three outgoing edges:
//!
//! - `if_true` applies when the predicate is true.
//! - `if_false` applies when the predicate is false.
//! - `if_uncertain` applies either way.
//!
//! Despite its name, `if_uncertain` does not mean that the predicate's value is unknown. It is a
//! "don't care" edge: the formula on that edge does not depend on the predicate. A node represents
//! this formula:
//!
//! ```text
//! if_uncertain OR (predicate AND if_true) OR (NOT predicate AND if_false)
//! ```
//!
//! The extra edge keeps repeated unions small. For example, `A OR B` can store `A` once on `B`'s
//! `if_uncertain` edge instead of copying `A` into both of `B`'s other edges.

use std::cmp::Ordering;

use ruff_index::{Idx, IndexVec};
use rustc_hash::FxHashMap;

use crate::ast_ids::ScopedUseId;
use crate::predicate::ScopedPredicateId;
use crate::rank::RankBitBox;
use crate::scope::FileScopeId;

/// The ID of a narrowing formula within one scope.
///
/// `ALWAYS_TRUE` means that no narrowing applies. `ALWAYS_FALSE` means that the path is
/// impossible.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct ScopedNarrowingConstraint(u32);

impl ScopedNarrowingConstraint {
    pub const ALWAYS_TRUE: Self = Self(u32::MAX);
    pub const ALWAYS_FALSE: Self = Self(u32::MAX - 1);

    pub fn is_terminal(self) -> bool {
        self.0 >= Self::ALWAYS_FALSE.0
    }
}

impl Idx for ScopedNarrowingConstraint {
    fn new(value: usize) -> Self {
        assert!(value <= Self::ALWAYS_FALSE.0 as usize);
        #[expect(clippy::cast_possible_truncation)]
        Self(value as u32)
    }

    fn index(self) -> usize {
        debug_assert!(!self.is_terminal());
        self.0 as usize
    }
}

const ALWAYS_TRUE: ScopedNarrowingConstraint = ScopedNarrowingConstraint::ALWAYS_TRUE;
const ALWAYS_FALSE: ScopedNarrowingConstraint = ScopedNarrowingConstraint::ALWAYS_FALSE;

/// Once a scope reaches this limit, operations return `ALWAYS_TRUE`. Dropping narrowing is less
/// precise, but avoids exponential growth on pathological input.
const MAX_INTERIOR_NODES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub struct InteriorNode {
    /// The predicate tested by this node.
    atom: ScopedPredicateId,
    /// The remaining formula when the predicate is true.
    if_true: ScopedNarrowingConstraint,
    /// The part of the formula that applies regardless of the predicate.
    if_uncertain: ScopedNarrowingConstraint,
    /// The remaining formula when the predicate is false.
    if_false: ScopedNarrowingConstraint,
}

impl InteriorNode {
    pub const fn atom(self) -> ScopedPredicateId {
        self.atom
    }

    pub const fn if_true(self) -> ScopedNarrowingConstraint {
        self.if_true
    }

    pub const fn if_uncertain(self) -> ScopedNarrowingConstraint {
        self.if_uncertain
    }

    pub const fn if_false(self) -> ScopedNarrowingConstraint {
        self.if_false
    }
}

#[derive(Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub struct NarrowingConstraints {
    used_interiors: Box<[InteriorNode]>,
    used_indices: Option<Box<RankBitBox>>,
}

impl NarrowingConstraints {
    pub fn get_interior_node(&self, id: ScopedNarrowingConstraint) -> InteriorNode {
        debug_assert!(!id.is_terminal());
        let raw_index = id.0 as usize;
        if let Some(used_indices) = &self.used_indices {
            debug_assert!(
                used_indices.get_bit(raw_index).unwrap_or(false),
                "all used narrowing constraints should have been marked as used",
            );
            self.used_interiors[used_indices.rank(raw_index) as usize]
        } else {
            self.used_interiors[raw_index]
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct NarrowingConstraintsBuilder {
    interiors: IndexVec<ScopedNarrowingConstraint, InteriorNode>,
    interior_used: IndexVec<ScopedNarrowingConstraint, bool>,
    interior_cache: FxHashMap<InteriorNode, ScopedNarrowingConstraint>,
    and_cache: FxHashMap<
        (ScopedNarrowingConstraint, ScopedNarrowingConstraint),
        ScopedNarrowingConstraint,
    >,
    or_cache: FxHashMap<
        (ScopedNarrowingConstraint, ScopedNarrowingConstraint),
        ScopedNarrowingConstraint,
    >,
    remove_predicate_cache:
        FxHashMap<(ScopedNarrowingConstraint, ScopedPredicateId), ScopedNarrowingConstraint>,
    bdd_or_cache: FxHashMap<
        (ScopedNarrowingConstraint, ScopedNarrowingConstraint),
        ScopedNarrowingConstraint,
    >,
    bdd_projection_cache: FxHashMap<ScopedNarrowingConstraint, ScopedNarrowingConstraint>,
}

impl NarrowingConstraintsBuilder {
    pub(crate) fn build(self) -> NarrowingConstraints {
        if self.interior_used.iter().all(|used| *used) {
            NarrowingConstraints {
                used_interiors: self.interiors.raw.into_boxed_slice(),
                used_indices: None,
            }
        } else {
            let used_indices = RankBitBox::from_bits(self.interior_used.iter().copied());
            let used_interiors = self
                .interiors
                .into_iter()
                .zip(self.interior_used)
                .filter_map(|(interior, used)| used.then_some(interior))
                .collect();
            NarrowingConstraints {
                used_interiors,
                used_indices: Some(Box::new(used_indices)),
            }
        }
    }

    pub(crate) fn mark_used(&mut self, node: ScopedNarrowingConstraint) {
        if !node.is_terminal() && !self.interior_used[node] {
            self.interior_used[node] = true;
            let node = self.interiors[node];
            self.mark_used(node.if_true);
            self.mark_used(node.if_uncertain);
            self.mark_used(node.if_false);
        }
    }

    fn cmp_atoms(&self, a: ScopedNarrowingConstraint, b: ScopedNarrowingConstraint) -> Ordering {
        if a == b || (a.is_terminal() && b.is_terminal()) {
            Ordering::Equal
        } else if a.is_terminal() {
            Ordering::Greater
        } else if b.is_terminal() {
            Ordering::Less
        } else {
            self.interiors[a]
                .atom
                .cmp(&self.interiors[b].atom)
                .reverse()
        }
    }

    fn add_interior(&mut self, node: InteriorNode) -> ScopedNarrowingConstraint {
        if node.if_uncertain == ALWAYS_TRUE {
            return ALWAYS_TRUE;
        }
        if node.if_true == node.if_false {
            return self.add_or_constraint(node.if_true, node.if_uncertain);
        }

        *self.interior_cache.entry(node).or_insert_with(|| {
            self.interior_used.push(false);
            self.interiors.push(node)
        })
    }

    pub(crate) fn add_atom(&mut self, predicate: ScopedPredicateId) -> ScopedNarrowingConstraint {
        if predicate == ScopedPredicateId::ALWAYS_FALSE {
            ALWAYS_FALSE
        } else if predicate == ScopedPredicateId::ALWAYS_TRUE {
            ALWAYS_TRUE
        } else {
            self.add_interior(InteriorNode {
                atom: predicate,
                if_true: ALWAYS_TRUE,
                if_uncertain: ALWAYS_FALSE,
                if_false: ALWAYS_FALSE,
            })
        }
    }

    pub(crate) fn add_negated_atom(
        &mut self,
        predicate: ScopedPredicateId,
    ) -> ScopedNarrowingConstraint {
        if predicate == ScopedPredicateId::ALWAYS_FALSE {
            ALWAYS_TRUE
        } else if predicate == ScopedPredicateId::ALWAYS_TRUE {
            ALWAYS_FALSE
        } else {
            self.add_interior(InteriorNode {
                atom: predicate,
                if_true: ALWAYS_FALSE,
                if_uncertain: ALWAYS_FALSE,
                if_false: ALWAYS_TRUE,
            })
        }
    }

    pub(crate) fn add_or_constraint(
        &mut self,
        a: ScopedNarrowingConstraint,
        b: ScopedNarrowingConstraint,
    ) -> ScopedNarrowingConstraint {
        match (a, b) {
            (ALWAYS_TRUE, _) | (_, ALWAYS_TRUE) => return ALWAYS_TRUE,
            (ALWAYS_FALSE, other) | (other, ALWAYS_FALSE) => return other,
            _ if a == b => return a,
            _ => {}
        }

        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.or_cache.get(&(a, b)) {
            return *cached;
        }
        if self.interiors.len() >= MAX_INTERIOR_NODES {
            return ALWAYS_TRUE;
        }

        let result = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];
                let if_true = self.add_or_constraint(a_node.if_true, b_node.if_true);
                let if_uncertain = self.add_or_constraint(a_node.if_uncertain, b_node.if_uncertain);
                let if_false = self.add_or_constraint(a_node.if_false, b_node.if_false);
                self.add_interior(InteriorNode {
                    atom: a_node.atom,
                    if_true,
                    if_uncertain,
                    if_false,
                })
            }
            Ordering::Less => {
                let node = self.interiors[a];
                let if_uncertain = self.add_or_constraint(node.if_uncertain, b);
                self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true: node.if_true,
                    if_uncertain,
                    if_false: node.if_false,
                })
            }
            Ordering::Greater => {
                let node = self.interiors[b];
                let if_uncertain = self.add_or_constraint(a, node.if_uncertain);
                self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true: node.if_true,
                    if_uncertain,
                    if_false: node.if_false,
                })
            }
        };

        self.or_cache.insert((a, b), result);
        result
    }

    /// Combines two formulas as an ordinary BDD, without creating `if_uncertain` edges.
    ///
    /// Other control-flow merges use this operation so they keep producing narrowed types in the
    /// same order as before. The `if`/`elif` continuation merge uses
    /// [`Self::add_or_constraint`] to avoid copying every earlier branch into every later one.
    pub(crate) fn add_bdd_or_constraint(
        &mut self,
        a: ScopedNarrowingConstraint,
        b: ScopedNarrowingConstraint,
    ) -> ScopedNarrowingConstraint {
        let a = self.project_to_bdd(a);
        let b = self.project_to_bdd(b);
        self.add_bdd_or(a, b)
    }

    pub(crate) fn add_and_constraint(
        &mut self,
        a: ScopedNarrowingConstraint,
        b: ScopedNarrowingConstraint,
    ) -> ScopedNarrowingConstraint {
        match (a, b) {
            (ALWAYS_FALSE, _) | (_, ALWAYS_FALSE) => return ALWAYS_FALSE,
            (ALWAYS_TRUE, other) | (other, ALWAYS_TRUE) => return other,
            _ if a == b => return a,
            _ => {}
        }

        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.and_cache.get(&(a, b)) {
            return *cached;
        }
        if self.interiors.len() >= MAX_INTERIOR_NODES {
            return ALWAYS_TRUE;
        }

        let result = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];

                let (if_true, if_uncertain, if_false) = if a_node.if_uncertain == ALWAYS_FALSE
                    && b_node.if_uncertain == ALWAYS_FALSE
                {
                    (
                        self.add_and_constraint(a_node.if_true, b_node.if_true),
                        ALWAYS_FALSE,
                        self.add_and_constraint(a_node.if_false, b_node.if_false),
                    )
                } else {
                    let b_true_or_uncertain =
                        self.add_or_constraint(b_node.if_true, b_node.if_uncertain);
                    let true_from_a = self.add_and_constraint(a_node.if_true, b_true_or_uncertain);
                    let true_from_uncertain =
                        self.add_and_constraint(a_node.if_uncertain, b_node.if_true);
                    let if_true = self.add_or_constraint(true_from_a, true_from_uncertain);

                    let if_uncertain =
                        self.add_and_constraint(a_node.if_uncertain, b_node.if_uncertain);

                    let b_false_or_uncertain =
                        self.add_or_constraint(b_node.if_false, b_node.if_uncertain);
                    let false_from_a =
                        self.add_and_constraint(a_node.if_false, b_false_or_uncertain);
                    let false_from_uncertain =
                        self.add_and_constraint(a_node.if_uncertain, b_node.if_false);
                    let if_false = self.add_or_constraint(false_from_a, false_from_uncertain);

                    (if_true, if_uncertain, if_false)
                };

                self.add_interior(InteriorNode {
                    atom: a_node.atom,
                    if_true,
                    if_uncertain,
                    if_false,
                })
            }
            Ordering::Less => {
                let node = self.interiors[a];
                let if_true = self.add_and_constraint(node.if_true, b);
                let if_uncertain = if node.if_uncertain == ALWAYS_FALSE {
                    ALWAYS_FALSE
                } else {
                    self.add_and_constraint(node.if_uncertain, b)
                };
                let if_false = self.add_and_constraint(node.if_false, b);
                self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true,
                    if_uncertain,
                    if_false,
                })
            }
            Ordering::Greater => {
                let node = self.interiors[b];
                let if_true = self.add_and_constraint(a, node.if_true);
                let if_uncertain = if node.if_uncertain == ALWAYS_FALSE {
                    ALWAYS_FALSE
                } else {
                    self.add_and_constraint(a, node.if_uncertain)
                };
                let if_false = self.add_and_constraint(a, node.if_false);
                self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true,
                    if_uncertain,
                    if_false,
                })
            }
        };

        self.and_cache.insert((a, b), result);
        result
    }

    /// Removes the listed predicates from a formula.
    ///
    /// The result is true whenever the original formula is true for either value of each removed
    /// predicate.
    pub(crate) fn remove_predicates(
        &mut self,
        mut constraint: ScopedNarrowingConstraint,
        predicates: &[ScopedPredicateId],
    ) -> ScopedNarrowingConstraint {
        for &predicate in predicates {
            constraint = self.remove_predicate(constraint, predicate);
        }
        constraint
    }

    fn remove_predicate(
        &mut self,
        constraint: ScopedNarrowingConstraint,
        predicate: ScopedPredicateId,
    ) -> ScopedNarrowingConstraint {
        if constraint.is_terminal() {
            return constraint;
        }
        if let Some(cached) = self.remove_predicate_cache.get(&(constraint, predicate)) {
            return *cached;
        }

        let node = self.interiors[constraint];
        let if_true = self.remove_predicate(node.if_true, predicate);
        let if_uncertain = self.remove_predicate(node.if_uncertain, predicate);
        let if_false = self.remove_predicate(node.if_false, predicate);
        let result = if node.atom == predicate {
            let either_branch = self.add_or_constraint(if_true, if_false);
            self.add_or_constraint(either_branch, if_uncertain)
        } else {
            self.add_interior(InteriorNode {
                atom: node.atom,
                if_true,
                if_uncertain,
                if_false,
            })
        };
        self.remove_predicate_cache
            .insert((constraint, predicate), result);
        result
    }

    pub(crate) fn are_equivalent(
        &mut self,
        a: ScopedNarrowingConstraint,
        b: ScopedNarrowingConstraint,
    ) -> bool {
        self.project_to_bdd(a) == self.project_to_bdd(b)
    }

    /// Expands the `if_uncertain` edge into both other edges, producing a canonical BDD.
    fn project_to_bdd(
        &mut self,
        constraint: ScopedNarrowingConstraint,
    ) -> ScopedNarrowingConstraint {
        match constraint {
            ALWAYS_TRUE | ALWAYS_FALSE => constraint,
            _ => {
                if let Some(cached) = self.bdd_projection_cache.get(&constraint) {
                    return *cached;
                }
                if self.interiors.len() >= MAX_INTERIOR_NODES {
                    return ALWAYS_TRUE;
                }

                let node = self.interiors[constraint];
                let if_true = self.project_to_bdd(node.if_true);
                let if_uncertain = self.project_to_bdd(node.if_uncertain);
                let if_false = self.project_to_bdd(node.if_false);
                let if_true = self.add_bdd_or(if_true, if_uncertain);
                let if_false = self.add_bdd_or(if_false, if_uncertain);
                let result = self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true,
                    if_uncertain: ALWAYS_FALSE,
                    if_false,
                });
                self.bdd_projection_cache.insert(constraint, result);
                result
            }
        }
    }

    /// Computes OR for formulas that have already been converted to ordinary BDDs.
    fn add_bdd_or(
        &mut self,
        a: ScopedNarrowingConstraint,
        b: ScopedNarrowingConstraint,
    ) -> ScopedNarrowingConstraint {
        match (a, b) {
            (ALWAYS_TRUE, _) | (_, ALWAYS_TRUE) => return ALWAYS_TRUE,
            (ALWAYS_FALSE, other) | (other, ALWAYS_FALSE) => return other,
            _ if a == b => return a,
            _ => {}
        }

        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.bdd_or_cache.get(&(a, b)) {
            return *cached;
        }
        if self.interiors.len() >= MAX_INTERIOR_NODES {
            return ALWAYS_TRUE;
        }

        let result = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];
                debug_assert_eq!(a_node.if_uncertain, ALWAYS_FALSE);
                debug_assert_eq!(b_node.if_uncertain, ALWAYS_FALSE);
                let if_true = self.add_bdd_or(a_node.if_true, b_node.if_true);
                let if_false = self.add_bdd_or(a_node.if_false, b_node.if_false);
                self.add_interior(InteriorNode {
                    atom: a_node.atom,
                    if_true,
                    if_uncertain: ALWAYS_FALSE,
                    if_false,
                })
            }
            Ordering::Less => {
                let node = self.interiors[a];
                debug_assert_eq!(node.if_uncertain, ALWAYS_FALSE);
                let if_true = self.add_bdd_or(node.if_true, b);
                let if_false = self.add_bdd_or(node.if_false, b);
                self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true,
                    if_uncertain: ALWAYS_FALSE,
                    if_false,
                })
            }
            Ordering::Greater => {
                let node = self.interiors[b];
                debug_assert_eq!(node.if_uncertain, ALWAYS_FALSE);
                let if_true = self.add_bdd_or(a, node.if_true);
                let if_false = self.add_bdd_or(a, node.if_false);
                self.add_interior(InteriorNode {
                    atom: node.atom,
                    if_true,
                    if_uncertain: ALWAYS_FALSE,
                    if_false,
                })
            }
        };
        self.bdd_or_cache.insert((a, b), result);
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintKey {
    NarrowingConstraint(ScopedNarrowingConstraint),
    NestedScope(FileScopeId),
    UseId(ScopedUseId),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn predicate(index: usize) -> ScopedPredicateId {
        ScopedPredicateId::new(index)
    }

    fn evaluate(
        constraints: &NarrowingConstraintsBuilder,
        constraint: ScopedNarrowingConstraint,
        values: &[bool],
    ) -> bool {
        match constraint {
            ALWAYS_TRUE => true,
            ALWAYS_FALSE => false,
            _ => {
                let node = constraints.interiors[constraint];
                evaluate(constraints, node.if_uncertain, values)
                    || if values[node.atom.index()] {
                        evaluate(constraints, node.if_true, values)
                    } else {
                        evaluate(constraints, node.if_false, values)
                    }
            }
        }
    }

    #[test]
    fn boolean_operations_match_their_truth_tables() {
        let mut constraints = NarrowingConstraintsBuilder::default();
        let a = constraints.add_atom(predicate(0));
        let b = constraints.add_atom(predicate(1));

        let a_or_b = constraints.add_or_constraint(a, b);
        let not_c = constraints.add_negated_atom(predicate(2));
        let formula = constraints.add_and_constraint(a_or_b, not_c);

        for mask in 0_u8..8 {
            let values = [mask & 0b001 != 0, mask & 0b010 != 0, mask & 0b100 != 0];
            assert_eq!(
                evaluate(&constraints, formula, &values),
                (values[0] || values[1]) && !values[2],
            );
        }
    }

    #[test]
    fn union_parks_the_other_operand_in_the_uncertain_branch() {
        let mut constraints = NarrowingConstraintsBuilder::default();
        let a = constraints.add_atom(predicate(0));
        let b = constraints.add_atom(predicate(1));

        let union = constraints.add_or_constraint(a, b);
        let root = constraints.interiors[union];

        assert_eq!(root.atom, predicate(1));
        assert_eq!(root.if_true, ALWAYS_TRUE);
        assert_eq!(root.if_uncertain, a);
        assert_eq!(root.if_false, ALWAYS_FALSE);
    }

    #[test]
    fn removing_a_predicate_accepts_either_value() {
        let mut constraints = NarrowingConstraintsBuilder::default();
        let b = constraints.add_atom(predicate(1));
        let not_a = constraints.add_negated_atom(predicate(0));
        let branch = constraints.add_and_constraint(not_a, b);

        let without_a = constraints.remove_predicates(branch, &[predicate(0)]);

        assert!(constraints.are_equivalent(without_a, b));
    }

    #[test]
    fn removing_a_predicate_is_only_safe_after_a_preceding_branch_reaches_the_merge() {
        let mut constraints = NarrowingConstraintsBuilder::default();
        let a = constraints.add_atom(predicate(0));
        let b = constraints.add_atom(predicate(1));
        let not_a = constraints.add_negated_atom(predicate(0));
        let later_branch = constraints.add_and_constraint(not_a, b);
        let without_a = constraints.remove_predicates(later_branch, &[predicate(0)]);

        let merged = constraints.add_or_constraint(a, later_branch);
        let merged_without_a = constraints.add_or_constraint(a, without_a);
        assert!(constraints.are_equivalent(merged, merged_without_a));

        assert!(!constraints.are_equivalent(later_branch, without_a));
    }
}
