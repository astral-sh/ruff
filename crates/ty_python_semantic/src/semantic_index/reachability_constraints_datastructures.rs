//! # Core data structures for recording reachability constraints.
//!
//! See [`crate::reachability_constraints`] for more details.

use std::cmp::Ordering;

use ruff_index::{Idx, IndexVec};
use rustc_hash::FxHashMap;

use crate::rank::RankBitBox;
use crate::semantic_index::predicate::ScopedPredicateId;

/// A ternary formula that defines under what conditions a binding is visible. (A ternary formula
/// is just like a boolean formula, but with `Ambiguous` as a third potential result. See the
/// module documentation for more details.)
///
/// The primitive atoms of the formula are [`super::predicate::Predicate`]s, which express some
/// property of the runtime state of the code that we are analyzing.
///
/// We assume that each atom has a stable value each time that the formula is evaluated. An atom
/// that resolves to `Ambiguous` might be true or false, and we can't tell which — but within that
/// evaluation, we assume that the atom has the _same_ unknown value each time it appears. That
/// allows us to perform simplifications like `A ∨ !A → true` and `A ∧ !A → false`.
///
/// That means that when you are constructing a formula, you might need to create distinct atoms
/// for a particular [`super::predicate::Predicate`], if your formula needs to consider how a
/// particular runtime property might be different at different points in the execution of the
/// program.
///
/// reachability constraints are normalized, so equivalent constraints are guaranteed to have equal
/// IDs.
#[derive(Clone, Copy, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ScopedReachabilityConstraintId(u32);

impl std::fmt::Debug for ScopedReachabilityConstraintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_tuple("ScopedReachabilityConstraintId");
        match *self {
            // We use format_args instead of rendering the strings directly so that we don't get
            // any quotes in the output: ScopedReachabilityConstraintId(AlwaysTrue) instead of
            // ScopedReachabilityConstraintId("AlwaysTrue").
            ALWAYS_TRUE => f.field(&format_args!("AlwaysTrue")),
            AMBIGUOUS => f.field(&format_args!("Ambiguous")),
            ALWAYS_FALSE => f.field(&format_args!("AlwaysFalse")),
            _ => f.field(&self.0),
        };
        f.finish()
    }
}

// Internal details:
//
// There are 3 terminals, with hard-coded constraint IDs: true, ambiguous, and false.
//
// _Atoms_ are the underlying Predicates, which are the variables that are evaluated by the
// ternary function.
//
// _Interior nodes_ provide the TDD structure for the formula. Interior nodes are stored in an
// arena Vec, with the constraint ID providing an index into the arena.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) struct InteriorNode {
    /// A "variable" that is evaluated as part of a TDD ternary function. For reachability
    /// constraints, this is a `Predicate` that represents some runtime property of the Python
    /// code that we are evaluating.
    atom: ScopedPredicateId,
    if_true: ScopedReachabilityConstraintId,
    if_ambiguous: ScopedReachabilityConstraintId,
    if_false: ScopedReachabilityConstraintId,
}

impl InteriorNode {
    pub(crate) const fn atom(self) -> ScopedPredicateId {
        self.atom
    }

    pub(crate) const fn if_true(self) -> ScopedReachabilityConstraintId {
        self.if_true
    }

    pub(crate) const fn if_ambiguous(self) -> ScopedReachabilityConstraintId {
        self.if_ambiguous
    }

    pub(crate) const fn if_false(self) -> ScopedReachabilityConstraintId {
        self.if_false
    }
}

impl ScopedReachabilityConstraintId {
    /// A special ID that is used for an "always true" / "always visible" constraint.
    pub(crate) const ALWAYS_TRUE: ScopedReachabilityConstraintId =
        ScopedReachabilityConstraintId(0xffff_ffff);

    /// A special ID that is used for an ambiguous constraint.
    pub(crate) const AMBIGUOUS: ScopedReachabilityConstraintId =
        ScopedReachabilityConstraintId(0xffff_fffe);

    /// A special ID that is used for an "always false" / "never visible" constraint.
    pub(crate) const ALWAYS_FALSE: ScopedReachabilityConstraintId =
        ScopedReachabilityConstraintId(0xffff_fffd);

    pub(crate) fn is_terminal(self) -> bool {
        self.0 >= SMALLEST_TERMINAL.0
    }

    pub(crate) fn as_u32(self) -> u32 {
        self.0
    }
}

impl Idx for ScopedReachabilityConstraintId {
    #[inline]
    fn new(value: usize) -> Self {
        assert!(value <= (SMALLEST_TERMINAL.0 as usize));
        #[expect(clippy::cast_possible_truncation)]
        Self(value as u32)
    }

    #[inline]
    fn index(self) -> usize {
        debug_assert!(!self.is_terminal());
        self.0 as usize
    }
}

// Rebind some constants locally so that we don't need as many qualifiers below.
const ALWAYS_TRUE: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::ALWAYS_TRUE;
const AMBIGUOUS: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::AMBIGUOUS;
const ALWAYS_FALSE: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::ALWAYS_FALSE;
const SMALLEST_TERMINAL: ScopedReachabilityConstraintId = ALWAYS_FALSE;

/// Maximum number of interior TDD nodes per scope. When exceeded, new constraint
/// operations return `AMBIGUOUS` to prevent exponential blowup on pathological inputs
/// (e.g., a 5000-line while loop with hundreds of if-branches). This can lead to less precise
/// reachability analysis and type narrowing.
const MAX_INTERIOR_NODES: usize = 512 * 1024;

/// A collection of reachability constraints for a given scope.
#[derive(Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ReachabilityConstraints {
    /// The interior TDD nodes that were marked as used when being built.
    used_interiors: Box<[InteriorNode]>,
    /// A bit vector indicating which interior TDD nodes were marked as used. This is indexed by
    /// the node's [`ScopedReachabilityConstraintId`]. The rank of the corresponding bit gives the
    /// index of that node in the `used_interiors` vector.
    used_indices: RankBitBox,
}

impl ReachabilityConstraints {
    /// Look up an interior node by its constraint ID.
    pub(crate) fn get_interior_node(&self, id: ScopedReachabilityConstraintId) -> InteriorNode {
        debug_assert!(!id.is_terminal());
        let raw_index = id.as_u32() as usize;
        debug_assert!(
            self.used_indices().get_bit(raw_index).unwrap_or(false),
            "all used reachability constraints should have been marked as used",
        );
        let index = self.used_indices().rank(raw_index) as usize;
        self.used_interiors()[index]
    }

    pub(crate) fn used_interiors(&self) -> &[InteriorNode] {
        &self.used_interiors
    }

    pub(crate) fn used_indices(&self) -> &RankBitBox {
        &self.used_indices
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ReachabilityConstraintsBuilder {
    interiors: IndexVec<ScopedReachabilityConstraintId, InteriorNode>,
    interior_used: IndexVec<ScopedReachabilityConstraintId, bool>,
    interior_cache: FxHashMap<InteriorNode, ScopedReachabilityConstraintId>,
    not_cache: FxHashMap<ScopedReachabilityConstraintId, ScopedReachabilityConstraintId>,
    and_cache: FxHashMap<
        (
            ScopedReachabilityConstraintId,
            ScopedReachabilityConstraintId,
        ),
        ScopedReachabilityConstraintId,
    >,
    or_cache: FxHashMap<
        (
            ScopedReachabilityConstraintId,
            ScopedReachabilityConstraintId,
        ),
        ScopedReachabilityConstraintId,
    >,
}

impl ReachabilityConstraintsBuilder {
    pub(crate) fn build(self) -> ReachabilityConstraints {
        let used_indices = RankBitBox::from_bits(self.interior_used.iter().copied());
        let used_interiors = (self.interiors.into_iter())
            .zip(self.interior_used)
            .filter_map(|(interior, used)| used.then_some(interior))
            .collect();
        ReachabilityConstraints {
            used_interiors,
            used_indices,
        }
    }

    /// Marks that a particular TDD node is used. This lets us throw away interior nodes that were
    /// only calculated for intermediate values, and which don't need to be included in the final
    /// built result.
    pub(crate) fn mark_used(&mut self, node: ScopedReachabilityConstraintId) {
        if !node.is_terminal() && !self.interior_used[node] {
            self.interior_used[node] = true;
            let node = self.interiors[node];
            self.mark_used(node.if_true);
            self.mark_used(node.if_ambiguous);
            self.mark_used(node.if_false);
        }
    }

    /// Implements the ordering that determines which level a TDD node appears at.
    ///
    /// Each interior node checks the value of a single variable (for us, a `Predicate`).
    /// TDDs are ordered such that every path from the root of the graph to the leaves must
    /// check each variable at most once, and must check each variable in the same order.
    ///
    /// We can choose any ordering that we want, as long as it's consistent — with the
    /// caveat that terminal nodes must always be last in the ordering, since they are the
    /// leaf nodes of the graph.
    ///
    /// We currently compare interior nodes by looking at the Salsa IDs of each variable's
    /// `Predicate`, since this is already available and easy to compare. We also _reverse_
    /// the comparison of those Salsa IDs. The Salsa IDs are assigned roughly sequentially
    /// while traversing the source code. Reversing the comparison means `Predicate`s that
    /// appear later in the source will tend to be placed "higher" (closer to the root) in
    /// the TDD graph. We have found empirically that this leads to smaller TDD graphs [1],
    /// since there are often repeated combinations of `Predicate`s from earlier in the
    /// file.
    ///
    /// [1]: https://github.com/astral-sh/ruff/pull/20098
    fn cmp_atoms(
        &self,
        a: ScopedReachabilityConstraintId,
        b: ScopedReachabilityConstraintId,
    ) -> Ordering {
        if a == b || (a.is_terminal() && b.is_terminal()) {
            Ordering::Equal
        } else if a.is_terminal() {
            Ordering::Greater
        } else if b.is_terminal() {
            Ordering::Less
        } else {
            // See https://github.com/astral-sh/ruff/pull/20098 for an explanation of why this
            // ordering is reversed.
            self.interiors[a]
                .atom
                .cmp(&self.interiors[b].atom)
                .reverse()
        }
    }

    /// Adds an interior node, ensuring that we always use the same reachability constraint ID for
    /// equal nodes.
    fn add_interior(&mut self, node: InteriorNode) -> ScopedReachabilityConstraintId {
        // If the true and false branches lead to the same node, we can override the ambiguous
        // branch to go there too. And this node is then redundant and can be reduced.
        if node.if_true == node.if_false {
            return node.if_true;
        }

        *self.interior_cache.entry(node).or_insert_with(|| {
            self.interior_used.push(false);
            self.interiors.push(node)
        })
    }

    /// Adds a new reachability constraint that checks a single [`super::predicate::Predicate`].
    ///
    /// [`ScopedPredicateId`]s are the “variables” that are evaluated by a TDD. A TDD variable has
    /// the same value no matter how many times it appears in the ternary formula that the TDD
    /// represents.
    ///
    /// However, we sometimes have to model how a `Predicate` can have a different runtime
    /// value at different points in the execution of the program. To handle this, you can take
    /// advantage of the fact that the [`super::predicate::Predicates`] arena does not deduplicate
    /// `Predicate`s. You can add a `Predicate` multiple times, yielding different
    /// `ScopedPredicateId`s, which you can then create separate TDD atoms for.
    pub(crate) fn add_atom(
        &mut self,
        predicate: ScopedPredicateId,
    ) -> ScopedReachabilityConstraintId {
        if predicate == ScopedPredicateId::ALWAYS_FALSE {
            ALWAYS_FALSE
        } else if predicate == ScopedPredicateId::ALWAYS_TRUE {
            ALWAYS_TRUE
        } else {
            self.add_interior(InteriorNode {
                atom: predicate,
                if_true: ALWAYS_TRUE,
                if_ambiguous: AMBIGUOUS,
                if_false: ALWAYS_FALSE,
            })
        }
    }

    /// Adds a new reachability constraint that is the ternary NOT of an existing one.
    pub(crate) fn add_not_constraint(
        &mut self,
        a: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
        if a == ALWAYS_TRUE {
            return ALWAYS_FALSE;
        } else if a == AMBIGUOUS {
            return AMBIGUOUS;
        } else if a == ALWAYS_FALSE {
            return ALWAYS_TRUE;
        }

        if let Some(cached) = self.not_cache.get(&a) {
            return *cached;
        }

        if self.interiors.len() >= MAX_INTERIOR_NODES {
            return AMBIGUOUS;
        }

        let a_node = self.interiors[a];
        let if_true = self.add_not_constraint(a_node.if_true);
        let if_ambiguous = self.add_not_constraint(a_node.if_ambiguous);
        let if_false = self.add_not_constraint(a_node.if_false);
        let result = self.add_interior(InteriorNode {
            atom: a_node.atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.not_cache.insert(a, result);
        result
    }

    /// Adds a new reachability constraint that is the ternary OR of two existing ones.
    pub(crate) fn add_or_constraint(
        &mut self,
        a: ScopedReachabilityConstraintId,
        b: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
        match (a, b) {
            (ALWAYS_TRUE, _) | (_, ALWAYS_TRUE) => return ALWAYS_TRUE,
            (ALWAYS_FALSE, other) | (other, ALWAYS_FALSE) => return other,
            (AMBIGUOUS, AMBIGUOUS) => return AMBIGUOUS,
            _ => {}
        }

        // OR is commutative, which lets us halve the cache requirements
        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.or_cache.get(&(a, b)) {
            return *cached;
        }

        if self.interiors.len() >= MAX_INTERIOR_NODES {
            return AMBIGUOUS;
        }

        let (atom, if_true, if_ambiguous, if_false) = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];
                let if_true = self.add_or_constraint(a_node.if_true, b_node.if_true);
                let if_false = self.add_or_constraint(a_node.if_false, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a_node.if_ambiguous, b_node.if_ambiguous)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Less => {
                let a_node = self.interiors[a];
                let if_true = self.add_or_constraint(a_node.if_true, b);
                let if_false = self.add_or_constraint(a_node.if_false, b);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a_node.if_ambiguous, b)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Greater => {
                let b_node = self.interiors[b];
                let if_true = self.add_or_constraint(a, b_node.if_true);
                let if_false = self.add_or_constraint(a, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a, b_node.if_ambiguous)
                };
                (b_node.atom, if_true, if_ambiguous, if_false)
            }
        };

        let result = self.add_interior(InteriorNode {
            atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.or_cache.insert((a, b), result);
        result
    }

    /// Adds a new reachability constraint that is the ternary AND of two existing ones.
    pub(crate) fn add_and_constraint(
        &mut self,
        a: ScopedReachabilityConstraintId,
        b: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
        match (a, b) {
            (ALWAYS_FALSE, _) | (_, ALWAYS_FALSE) => return ALWAYS_FALSE,
            (ALWAYS_TRUE, other) | (other, ALWAYS_TRUE) => return other,
            (AMBIGUOUS, AMBIGUOUS) => return AMBIGUOUS,
            _ => {}
        }

        // AND is commutative, which lets us halve the cache requirements
        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.and_cache.get(&(a, b)) {
            return *cached;
        }

        if self.interiors.len() >= MAX_INTERIOR_NODES {
            return AMBIGUOUS;
        }

        let (atom, if_true, if_ambiguous, if_false) = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];
                let if_true = self.add_and_constraint(a_node.if_true, b_node.if_true);
                let if_false = self.add_and_constraint(a_node.if_false, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a_node.if_ambiguous, b_node.if_ambiguous)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Less => {
                let a_node = self.interiors[a];
                let if_true = self.add_and_constraint(a_node.if_true, b);
                let if_false = self.add_and_constraint(a_node.if_false, b);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a_node.if_ambiguous, b)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Greater => {
                let b_node = self.interiors[b];
                let if_true = self.add_and_constraint(a, b_node.if_true);
                let if_false = self.add_and_constraint(a, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a, b_node.if_ambiguous)
                };
                (b_node.atom, if_true, if_ambiguous, if_false)
            }
        };

        let result = self.add_interior(InteriorNode {
            atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.and_cache.insert((a, b), result);
        result
    }
}
