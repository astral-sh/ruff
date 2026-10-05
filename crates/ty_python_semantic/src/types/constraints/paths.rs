//! [`PathAssignments`] and friends

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::ops::{ControlFlow, Range};

use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use std::convert::Infallible;

use ruff_index::{IndexVec, newtype_index};

use super::control::{
    AllocationKind, PathAdvance, PathReserve, PathTable, PathTypevarSet, PathWork, TddControl,
    TddError, TddWork, Unrestricted, admit_path_work, reserve_vec, sequence_growth, unrestricted,
};
use super::independent_pair_skip_with;
use super::satisfaction::OrdinarySatisfaction;
use super::variables::TypeVarEquivalenceBound;
use crate::types::constraints::sequents::{
    Sequent, SequentGroup, SequentMap, sequent_fuel_cost_from_depths,
};
use crate::types::constraints::variables::Constraint;
use crate::types::constraints::{
    ConstraintAssignment, ConstraintId, ConstraintSetStorage, InteriorNode, InteriorNodeData, Node,
    NodeId, PathVisitor, SourceOrderId, TypeVarId,
};
use crate::{Db, FxIndexMap, ProgramEnvironment};

/// The position of an assignment in insertion order.
#[newtype_index]
struct AssignmentIndex;

/// The collection of constraints that we know to be true or false at a certain point when
/// traversing a BDD.
///
/// An important part of this traversal is that not all of those constraints come directly from the
/// BDD, since constraints are not independent. In particular, there can be "implications", which
/// record e.g. when two constraints both being true imply another:
/// `A ≤ list[B] ∧ B ≤ int → A ≤ list[int]`. If we see `A ≤ list[B]` and `B ≤ int` in a BDD path,
/// we can _assume_ that `A ≤ list[int]` also holds, even if it doesn't actually appear in the BDD.
///
/// Unfortunately, there are certain implications that are technically true, but not helpful;
/// for instance, because they cause us to endlessly expand a constraint by substituting a bound
/// into itself.
///
/// We use a "fuel" mechanism to prevent these kinds of situations, without having to play
/// whack-a-mole to implement detection patterns for all of the pathological patterns. Each
/// derived constraint costs at least one unit of fuel. Nested typevars increase that cost according
/// to their depth, as does any constructor depth introduced relative to the antecedents. Measuring
/// structural growth instead of absolute depth ensures that propagating an existing complex
/// concrete bound remains cheap, while repeatedly wrapping that bound continues to consume path
/// fuel after no nested typevars remain.
///
/// We track this fuel in two ways: First, there is a global limit on the total amount of work we
/// are willing to do for a particular BDD path traversal. Second, there is a more focused
/// "per-path" limit, which records how far removed a derived constraint is from a constraint that
/// actually appears in the BDD. If either of those limits are exceeded, we ignore the derived
/// constraint that we are currently considering.
#[derive(Debug)]
pub(crate) struct PathAssignments {
    /// All of the rules that we know for inferring derived constraints on the current path.
    sequents: Vec<Sequent<ConstraintId, u16>>,
    /// Each assignment's source constraint and greatest remaining per-path fuel.
    pub(super) assignments: FxIndexMap<ConstraintAssignment, (ConstraintId, u16)>,
    /// Positions in `assignments`, cleared when their branch is left. Fuel stays in the map so
    /// replenishment and rollback do not need to update these indices.
    positive_assignment_indices: IndexVec<ConstraintId, Option<AssignmentIndex>>,
    negative_assignment_indices: IndexVec<ConstraintId, Option<AssignmentIndex>>,
    /// Previous fuel values, keyed by assignment index, for rolling back replenishments when
    /// leaving a BDD branch. Keeping the maximum in `assignments` makes fuel lookups constant-time.
    fuel_undo: Vec<(usize, u16)>,
    /// The amount of global fuel that remains across all assignments and paths.
    remaining_overall_fuel: u16,
    /// Constraints that we have discovered, mapped to whether we have processed them yet. (This
    /// ensures a stable order for all of the derived constraints that we create, while still
    /// letting us create them lazily.)
    discovered: FxIndexMap<ConstraintId, bool>,
    /// Constraint pairs that we have already checked and added to `sequents`.
    elaborated_pairs: FxHashSet<(ConstraintId, ConstraintId)>,

    /// Consequents grouped by the discovery call that introduced their sequents.
    single_replay_consequents: FxHashMap<ConstraintId, Vec<ConstraintId>>,
    pair_replay_consequents: FxHashMap<(ConstraintId, ConstraintId), Vec<ConstraintId>>,

    /// Type variables that only involve concrete constraints and so do not participate in sequent
    /// discovery.
    independent_typevars: FxHashSet<TypeVarId>,

    /// Derived assignments that have been queued up to be added to the current path.
    assignment_queue: VecDeque<(ConstraintAssignment, AssignmentFuel)>,

    /// The next chunk of derived assignments that have been queued up to add to the current path.
    /// If we derive the same assignment multiple times, we keep the derivation that lets us make
    /// the most additional progress (more remaining fuel for this derivation chain, less overall
    /// fuel consumed).
    new_assignments: FxIndexMap<ConstraintAssignment, AssignmentFuel>,
}

/// The total amount of fuel that we are willing to spend for this path traversal. This was
/// chosen empirically, to balance performance with accurate ecosystem diagnostics.
const OVERALL_FUEL_BUDGET: u16 = 256;

/// The maximum number of "trips through the sequent map" that we are willing to take for a
/// derived constraint. This records how far removed we are from a constraint that comes
/// directly from the BDD.
const PATH_FUEL_BUDGET: u16 = 8;

/// The fuel cost of deriving a particular assignment during BDD path walking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AssignmentFuel {
    /// The amount of fuel consumed when deriving the assignment, or None if this assignment came
    /// directly from the BDD
    consumed: Option<u16>,
    /// The amount of fuel remaining on the derivation path after deriving this assignment
    remaining: u16,
}

impl AssignmentFuel {
    fn origin() -> AssignmentFuel {
        AssignmentFuel {
            consumed: None,
            remaining: PATH_FUEL_BUDGET,
        }
    }

    fn derived(consumed: u16, remaining: u16) -> AssignmentFuel {
        AssignmentFuel {
            consumed: Some(consumed),
            remaining,
        }
    }

    fn is_derived(self) -> bool {
        self.consumed.is_some()
    }
}

impl PartialOrd for AssignmentFuel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AssignmentFuel {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_key = (self.remaining, std::cmp::Reverse(self.consumed));
        let other_key = (other.remaining, std::cmp::Reverse(other.consumed));
        self_key.cmp(&other_key)
    }
}

/// The path-local state to restore after an edge, including an interrupted subtree.
struct EdgeCheckpoint {
    assignments_start: usize,
    fuel_undo_start: usize,
    remaining_overall_fuel: u16,
}

struct EdgeOutcome {
    checkpoint: EdgeCheckpoint,
    new_range: Range<usize>,
    found_conflict: bool,
}

enum PathVisitStep<B, R> {
    Node(NodeId),
    Impossible,
    Return(ControlFlow<B, R>),
}

/// Completed siblings stay in traversal order while the next subtree is visited.
enum PathVisitPhase<R> {
    True,
    Uncertain { if_true: R },
    False { if_true: R, if_uncertain: R },
}

pub(super) struct PathVisitFrame<V: PathVisitor> {
    interior: InteriorNodeData,
    interior_value: V::Interior,
    phase: PathVisitPhase<V::Result>,
    /// Negated traversal reports the absent uncertain edge without changing the path.
    checkpoint: Option<EdgeCheckpoint>,
    new_range: Range<usize>,
}

impl PathAssignments {
    #[cfg(test)]
    pub(super) fn observed_sequents(&self) -> &[Sequent<ConstraintId, u16>] {
        &self.sequents
    }

    #[cfg(test)]
    pub(super) fn observed_discovered(&self) -> impl Iterator<Item = (ConstraintId, bool)> + '_ {
        self.discovered
            .iter()
            .map(|(&id, &processed)| (id, processed))
    }

    fn empty() -> Self {
        Self {
            sequents: Vec::new(),
            assignments: FxIndexMap::default(),
            positive_assignment_indices: IndexVec::new(),
            negative_assignment_indices: IndexVec::new(),
            fuel_undo: Vec::new(),
            discovered: FxIndexMap::default(),
            elaborated_pairs: FxHashSet::default(),
            single_replay_consequents: FxHashMap::default(),
            pair_replay_consequents: FxHashMap::default(),
            independent_typevars: FxHashSet::default(),
            remaining_overall_fuel: OVERALL_FUEL_BUDGET,
            assignment_queue: VecDeque::new(),
            new_assignments: FxIndexMap::default(),
        }
    }

    /// Orders projected facts by replaying the rules already discovered during this walk.
    ///
    /// Projection emits derived facts in TDD branch order. Retaining that order can prevent
    /// recursive relations from converging when an equivalent diagram is rebuilt in a different
    /// arena. Start with the original source order and visit each rule's consequences in order,
    /// including intermediate facts that are themselves projected away. This only replays cached
    /// rules; it does not derive more facts or change the walk's assignments and fuel.
    pub(super) fn projection_source_order(
        &self,
        storage: &mut ConstraintSetStorage<'_>,
        original_source_order: Option<SourceOrderId>,
        derived_source_order: Option<SourceOrderId>,
    ) -> Option<SourceOrderId> {
        let emitted = storage.calculate_source_orders(derived_source_order);
        if emitted.is_empty() {
            return None;
        }
        let mut ordered = storage.calculate_source_orders(original_source_order);
        ordered.retain(|constraint| self.discovered.contains_key(constraint));
        let mut index = 0;
        // Once all emitted facts have positions, later appends cannot change their relative order.
        while !emitted.is_subset(&ordered)
            && let Some(constraint) = ordered.get_index(index).copied()
        {
            if self.discovered.get(&constraint) == Some(&true) {
                if let Some(consequents) = self.single_replay_consequents.get(&constraint) {
                    ordered.extend(
                        consequents
                            .iter()
                            .copied()
                            .filter(|constraint| self.discovered.contains_key(constraint)),
                    );
                }
            }
            for earlier_index in 0..index {
                let earlier = ordered[earlier_index];
                // Pair rules are not commutative. Replay the orientation used by this walk,
                // which can differ from the order in which the replay reaches its inputs.
                let pair = [(earlier, constraint), (constraint, earlier)]
                    .into_iter()
                    .find(|pair| self.elaborated_pairs.contains(pair));
                if let Some(consequents) =
                    pair.and_then(|pair| self.pair_replay_consequents.get(&pair))
                {
                    ordered.extend(
                        consequents
                            .iter()
                            .copied()
                            .filter(|constraint| self.discovered.contains_key(constraint)),
                    );
                }
            }
            index += 1;
        }
        debug_assert!(emitted.is_subset(&ordered));
        ordered
            .into_iter()
            .filter(|constraint| emitted.contains(constraint))
            .fold(None, |source_order, constraint| {
                let next = storage.constraint_source_order(constraint);
                storage.ordered_source_order(source_order, Some(next))
            })
    }

    pub(super) fn new(
        constraints: impl IntoIterator<Item = ConstraintId>,
        independent_typevars: FxHashSet<TypeVarId>,
    ) -> Self {
        let discovered = constraints
            .into_iter()
            .map(|constraint| (constraint, false))
            .collect();
        Self {
            discovered,
            independent_typevars,
            ..Self::empty()
        }
    }

    pub(super) fn visit<'db, V>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        visitor: &mut V,
    ) -> ControlFlow<V::Break, V::Result>
    where
        V: PathVisitor,
    {
        let mut guard = BorrowedPathVisit::new(self, node, false);
        match path_visit_body_sync(
            guard.state_mut(),
            visitor,
            &mut OrdinarySatisfaction { db, env, storage },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Visits the paths of the negation of `node`, without constructing that negation eagerly.
    pub(super) fn visit_negated<'db, V>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        visitor: &mut V,
    ) -> ControlFlow<V::Break, V::Result>
    where
        V: PathVisitor,
    {
        let mut guard = BorrowedPathVisit::new(self, node, true);
        match path_visit_body_sync(
            guard.state_mut(),
            visitor,
            &mut OrdinarySatisfaction { db, env, storage },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Walks one of the outgoing edges of an internal BDD node. `assignment` describes the
    /// constraint that the BDD node checks, and whether we are following the `if_true` or
    /// `if_false` edge.
    ///
    /// This new assignment might cause this path to become impossible — for instance, if we were
    /// already assuming (from an earlier edge in the path) a constraint that is disjoint with this
    /// one. We might also be able to infer _other_ assignments that do not appear in the BDD
    /// directly, but which are implied from a combination of constraints that we _have_ seen.
    ///
    /// The callback receives whether the path has become impossible, so it can report that
    /// outcome instead of continuing into the subtree.
    ///
    /// Your callback will also be provided a slice of all of the constraints that we were able to
    /// infer from `assignment` combined with the information we already knew. (For borrow-check
    /// reasons, we provide this as a [`Range`]; use that range to index into `self.assignments` to
    /// get the list of all of the assignments that we learned from this edge.)
    ///
    /// You will presumably end up making a recursive call of some kind to keep progressing through
    /// the BDD. You should make this call from inside of your callback, so that as you get further
    /// down into the BDD structure, we remember all of the information that we have learned from
    /// the path we're on.
    pub(super) fn walk_edge<'db, R>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        assignment: ConstraintAssignment,
        f: impl FnOnce(&mut ConstraintSetStorage<'db>, &mut Self, Range<usize>, bool) -> R,
    ) -> R {
        let edge = match path_enter_edge_sync(
            self,
            assignment,
            &mut OrdinarySatisfaction { db, env, storage },
        ) {
            Ok(edge) => edge,
            Err(never) => match never {},
        };
        let result = f(storage, self, edge.new_range, edge.found_conflict);
        self.restore_edge(&edge.checkpoint);
        result
    }

    fn restore_edge(&mut self, checkpoint: &EdgeCheckpoint) {
        // Reset back to where we were before following this edge, so that the caller can reuse a
        // single instance for the entire BDD traversal.
        self.assignment_queue.clear();
        // A branch can replenish an assignment more than once. Restore in reverse order while
        // every referenced assignment still exists.
        for (index, previous_fuel) in self.fuel_undo.drain(checkpoint.fuel_undo_start..).rev() {
            self.assignments[index].1 = previous_fuel;
        }
        for assignment in self.assignments[checkpoint.assignments_start..].keys() {
            match *assignment {
                ConstraintAssignment::Positive(constraint) => {
                    self.positive_assignment_indices[constraint] = None;
                }
                ConstraintAssignment::Negative(constraint) => {
                    self.negative_assignment_indices[constraint] = None;
                }
                ConstraintAssignment::Unconstrained(_) => {}
            }
        }
        self.assignments.truncate(checkpoint.assignments_start);
        self.remaining_overall_fuel = checkpoint.remaining_overall_fuel;
    }

    pub(super) fn positive_constraints(
        &self,
    ) -> impl Iterator<Item = (ConstraintId, ConstraintId)> + '_ {
        self.assignments.iter().filter_map(
            |(assignment, (source_constraint, _))| match assignment {
                ConstraintAssignment::Positive(constraint) => {
                    Some((*constraint, *source_constraint))
                }
                ConstraintAssignment::Negative(_) | ConstraintAssignment::Unconstrained(_) => None,
            },
        )
    }

    fn assignment_holds(&self, assignment: ConstraintAssignment) -> bool {
        self.assignment_index(assignment).is_some()
    }

    fn assignment_index(&self, assignment: ConstraintAssignment) -> Option<usize> {
        let indices = match assignment {
            ConstraintAssignment::Positive(_) => &self.positive_assignment_indices,
            ConstraintAssignment::Negative(_) => &self.negative_assignment_indices,
            ConstraintAssignment::Unconstrained(_) => {
                return self.assignments.get_index_of(&assignment);
            }
        };
        indices
            .get(assignment.constraint())
            .copied()
            .flatten()
            .map(AssignmentIndex::as_usize)
    }

    fn record_assignment_index(&mut self, assignment: ConstraintAssignment, index: usize) {
        let indices = match assignment {
            ConstraintAssignment::Positive(_) => &mut self.positive_assignment_indices,
            ConstraintAssignment::Negative(_) => &mut self.negative_assignment_indices,
            ConstraintAssignment::Unconstrained(_) => return,
        };
        let constraint = assignment.constraint();
        if indices.len() <= constraint.as_usize() {
            indices.resize(constraint.as_usize() + 1, None);
        }
        indices[constraint] = Some(AssignmentIndex::from_usize(index));
    }

    fn contains_constraint(&self, constraint: ConstraintId) -> bool {
        self.assignment_holds(constraint.when_true())
            || self.assignment_holds(constraint.when_false())
            || self.assignment_holds(constraint.when_unconstrained())
    }

    /// Returns the greatest remaining fuel for any derivation of `assignment` on this path.
    fn max_remaining_fuel_for(&self, assignment: ConstraintAssignment) -> Option<u16> {
        self.assignment_index(assignment)
            .map(|index| self.assignments[index].1)
    }

    fn enqueue_assignment(&mut self, assignment: ConstraintAssignment, new_fuel: AssignmentFuel) {
        self.new_assignments
            .entry(assignment)
            .and_modify(|existing_fuel| {
                *existing_fuel = std::cmp::max(*existing_fuel, new_fuel);
            })
            .or_insert(new_fuel);
    }
}

pub(super) trait PathEffects<'db> {
    type Error;

    async fn checkpoint(&mut self, work: PathWork) -> Result<(), Self::Error>;
    async fn interior_data(&mut self, node: NodeId) -> Result<InteriorNodeData, Self::Error>;
    async fn constraint_data(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Self::Error>;
    async fn single_sequents(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<&'db SequentMap<'db>, Self::Error>;
    async fn pair_sequents(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> Result<&'db SequentMap<'db>, Self::Error>;
    async fn pair_cannot_produce(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> Result<bool, Self::Error>;
    async fn independent_pair_skip(
        &mut self,
        existing: ConstraintId,
        current: ConstraintId,
        independent: &FxHashSet<TypeVarId>,
    ) -> Result<bool, Self::Error>;
    async fn group_imports_left_first(
        &mut self,
        equivalence: TypeVarEquivalenceBound<'db>,
    ) -> Result<bool, Self::Error>;
    async fn intern_constraint(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<ConstraintId, Self::Error>;
    async fn constraint_depth(&mut self, id: ConstraintId) -> Result<(u16, u16), Self::Error>;
    async fn reflexive_constraint(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<bool, Self::Error>;
    async fn trace_path(
        &mut self,
        event: PathTrace,
        path: &PathAssignments,
    ) -> Result<(), Self::Error>;
    async fn reserve_path(
        &mut self,
        path: &mut PathAssignments,
        request: PathReserve,
    ) -> Result<(), Self::Error>;
    async fn reserve_replay(&mut self, ids: &mut Vec<ConstraintId>) -> Result<(), Self::Error>;
}

pub(super) trait PathVisitEffects<'db, V: PathVisitor>: PathEffects<'db> {
    async fn visit_node(&mut self, visitor: &mut V) -> Result<ControlFlow<V::Break>, Self::Error>;
    async fn visit_satisfied(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    async fn visit_unsatisfied(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    async fn visit_impossible(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    async fn enter_interior(
        &mut self,
        visitor: &mut V,
        interior: InteriorNode,
    ) -> Result<ControlFlow<V::Break, V::Interior>, Self::Error>;
    async fn visit_edge(
        &mut self,
        visitor: &mut V,
        interior_value: &V::Interior,
        subtree: V::Result,
        path: &PathAssignments,
        new_range: Range<usize>,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    async fn leave_interior(
        &mut self,
        visitor: &mut V,
        interior_value: &V::Interior,
        if_true: V::Result,
        if_uncertain: V::Result,
        if_false: V::Result,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    async fn or_nodes(&mut self, left: NodeId, right: NodeId) -> Result<NodeId, Self::Error>;
    async fn reserve_frames(
        &mut self,
        frames: &mut Vec<PathVisitFrame<V>>,
    ) -> Result<(), Self::Error>;
}

pub(super) trait SyncPathEffects<'db> {
    type Error;

    fn checkpoint(&mut self, work: PathWork) -> Result<(), Self::Error>;
    fn interior_data(&mut self, node: NodeId) -> Result<InteriorNodeData, Self::Error>;
    fn constraint_data(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Self::Error>;
    fn single_sequents(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<&'db SequentMap<'db>, Self::Error>;
    fn pair_sequents(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> Result<&'db SequentMap<'db>, Self::Error>;
    fn pair_cannot_produce(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> Result<bool, Self::Error>;
    fn independent_pair_skip(
        &mut self,
        existing: ConstraintId,
        current: ConstraintId,
        independent: &FxHashSet<TypeVarId>,
    ) -> Result<bool, Self::Error>;
    fn group_imports_left_first(
        &mut self,
        equivalence: TypeVarEquivalenceBound<'db>,
    ) -> Result<bool, Self::Error>;
    fn intern_constraint(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<ConstraintId, Self::Error>;
    fn constraint_depth(&mut self, id: ConstraintId) -> Result<(u16, u16), Self::Error>;
    fn reflexive_constraint(&mut self, constraint: Constraint<'db>) -> Result<bool, Self::Error>;
    fn trace_path(&mut self, event: PathTrace, path: &PathAssignments) -> Result<(), Self::Error>;
    fn reserve_path(
        &mut self,
        path: &mut PathAssignments,
        request: PathReserve,
    ) -> Result<(), Self::Error>;
    fn reserve_replay(&mut self, ids: &mut Vec<ConstraintId>) -> Result<(), Self::Error>;
}

pub(super) trait SyncPathVisitEffects<'db, V: PathVisitor>: SyncPathEffects<'db> {
    fn visit_node(&mut self, visitor: &mut V) -> Result<ControlFlow<V::Break>, Self::Error>;
    fn visit_satisfied(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    fn visit_unsatisfied(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    fn visit_impossible(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    fn enter_interior(
        &mut self,
        visitor: &mut V,
        interior: InteriorNode,
    ) -> Result<ControlFlow<V::Break, V::Interior>, Self::Error>;
    fn visit_edge(
        &mut self,
        visitor: &mut V,
        interior_value: &V::Interior,
        subtree: V::Result,
        path: &PathAssignments,
        new_range: Range<usize>,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    fn leave_interior(
        &mut self,
        visitor: &mut V,
        interior_value: &V::Interior,
        if_true: V::Result,
        if_uncertain: V::Result,
        if_false: V::Result,
    ) -> Result<ControlFlow<V::Break, V::Result>, Self::Error>;
    fn or_nodes(&mut self, left: NodeId, right: NodeId) -> Result<NodeId, Self::Error>;
    fn reserve_frames(&mut self, frames: &mut Vec<PathVisitFrame<V>>) -> Result<(), Self::Error>;
}

pub(super) struct PathVisitState {
    path: PathAssignments,
    node: NodeId,
    negated: bool,
}

pub(super) struct PathVisitCompletion<V: PathVisitor> {
    pub(super) path: PathAssignments,
    pub(super) flow: ControlFlow<V::Break, V::Result>,
}

struct BorrowedPathVisit<'path> {
    slot: &'path mut PathAssignments,
    state: PathVisitState,
}

impl<'path> BorrowedPathVisit<'path> {
    fn new(slot: &'path mut PathAssignments, node: NodeId, negated: bool) -> Self {
        let path = std::mem::replace(slot, PathAssignments::empty());
        Self {
            slot,
            state: PathVisitState {
                path,
                node,
                negated,
            },
        }
    }

    fn state_mut(&mut self) -> &mut PathVisitState {
        &mut self.state
    }
}

impl Drop for BorrowedPathVisit<'_> {
    fn drop(&mut self) {
        // Preserve the current partial path on panic; edge rollback occurs only in
        // the normal traversal. The replacement and overwritten slot are empty.
        *self.slot = std::mem::replace(&mut self.state.path, PathAssignments::empty());
    }
}

#[derive(Clone, Copy)]
pub(super) enum PathTrace {
    EnterEdge {
        assignment: ConstraintAssignment,
        assignments_start: usize,
    },
    NewAssignments {
        assignments_start: usize,
    },
    AssignmentConflict {
        assignment: ConstraintAssignment,
    },
    SingleConflict {
        ante: ConstraintId,
    },
    PairConflict {
        ante1: ConstraintId,
        ante2: ConstraintId,
    },
    TripleConflict {
        ante1: ConstraintId,
        ante2: ConstraintId,
        ante3: ConstraintId,
    },
}

#[ty_mapping_probe_macros::dual_satisfaction]
pub(super) async fn path_visit_owned_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
    path: PathAssignments,
    node: NodeId,
    visitor: &mut V,
    negated: bool,
    effects: &mut E,
) -> Result<PathVisitCompletion<V>, E::Error> {
    let mut state = PathVisitState {
        path,
        node,
        negated,
    };
    let flow = path_visit_body_with(&mut state, visitor, effects).await?;
    Ok(PathVisitCompletion {
        path: state.path,
        flow,
    })
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_visit_body_with<'db, V: PathVisitor, E: PathVisitEffects<'db, V>>(
    state: &mut PathVisitState,
    visitor: &mut V,
    effects: &mut E,
) -> Result<ControlFlow<V::Break, V::Result>, E::Error> {
    let node = state.node;
    let negated = state.negated;
    let path = &mut state.path;
    let mut frames: Vec<PathVisitFrame<V>> = Vec::new();
    let mut step = PathVisitStep::Node(node);
    loop {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::Traversal))
            .await?;
        step = match step {
            PathVisitStep::Node(node) => {
                if let ControlFlow::Break(b) = effects.visit_node(visitor).await? {
                    step = PathVisitStep::Return(ControlFlow::Break(b));
                    continue;
                }
                match node.node() {
                    Node::AlwaysTrue if negated => {
                        PathVisitStep::Return(effects.visit_unsatisfied(visitor, path).await?)
                    }
                    Node::AlwaysTrue => {
                        PathVisitStep::Return(effects.visit_satisfied(visitor, path).await?)
                    }
                    Node::AlwaysFalse if negated => {
                        PathVisitStep::Return(effects.visit_satisfied(visitor, path).await?)
                    }
                    Node::AlwaysFalse => {
                        PathVisitStep::Return(effects.visit_unsatisfied(visitor, path).await?)
                    }
                    Node::Interior(interior) => {
                        let interior_value = match effects.enter_interior(visitor, interior).await?
                        {
                            ControlFlow::Continue(value) => value,
                            ControlFlow::Break(b) => {
                                step = PathVisitStep::Return(ControlFlow::Break(b));
                                continue;
                            }
                        };
                        let interior = effects.interior_data(node).await?;
                        let subtree = if negated {
                            effects
                                .or_nodes(interior.if_true, interior.if_uncertain)
                                .await?
                        } else {
                            interior.if_true
                        };
                        let edge =
                            path_enter_edge_with(path, interior.constraint.when_true(), effects)
                                .await?;
                        effects.checkpoint(PathWork::FramePush).await?;
                        effects.reserve_frames(&mut frames).await?;
                        frames.push(PathVisitFrame {
                            interior,
                            interior_value,
                            phase: PathVisitPhase::True,
                            checkpoint: Some(edge.checkpoint),
                            new_range: edge.new_range,
                        });
                        if edge.found_conflict {
                            PathVisitStep::Impossible
                        } else {
                            PathVisitStep::Node(subtree)
                        }
                    }
                }
            }
            PathVisitStep::Impossible => {
                PathVisitStep::Return(effects.visit_impossible(visitor, path).await?)
            }
            PathVisitStep::Return(ControlFlow::Break(b)) => {
                // Match recursive unwinding: restore every active edge, without calling
                // the remaining visitor callbacks or discarding rules discovered on it.
                for frame in frames.into_iter().rev() {
                    if let Some(checkpoint) = frame.checkpoint {
                        effects
                            .checkpoint(PathWork::RestoreEdge {
                                assignments: path.assignments.len(),
                                retained_assignments: checkpoint.assignments_start,
                                undo: path.fuel_undo.len(),
                                retained_undo: checkpoint.fuel_undo_start,
                                queued: path.assignment_queue.len(),
                                assignment_capacity: path.assignments.capacity(),
                            })
                            .await?;
                        path.restore_edge(&checkpoint);
                    }
                }
                return Ok(ControlFlow::Break(b));
            }
            PathVisitStep::Return(ControlFlow::Continue(subtree)) => {
                effects.checkpoint(PathWork::FramePop).await?;
                let Some(frame) = frames.pop() else {
                    return Ok(ControlFlow::Continue(subtree));
                };
                let PathVisitFrame {
                    interior,
                    interior_value,
                    phase,
                    checkpoint,
                    new_range,
                } = frame;
                let result = effects
                    .visit_edge(visitor, &interior_value, subtree, path, new_range)
                    .await?;
                if let Some(checkpoint) = checkpoint {
                    effects
                        .checkpoint(PathWork::RestoreEdge {
                            assignments: path.assignments.len(),
                            retained_assignments: checkpoint.assignments_start,
                            undo: path.fuel_undo.len(),
                            retained_undo: checkpoint.fuel_undo_start,
                            queued: path.assignment_queue.len(),
                            assignment_capacity: path.assignments.capacity(),
                        })
                        .await?;
                    path.restore_edge(&checkpoint);
                }
                let result = match result {
                    ControlFlow::Continue(result) => result,
                    ControlFlow::Break(b) => {
                        step = PathVisitStep::Return(ControlFlow::Break(b));
                        continue;
                    }
                };
                let (phase, assignment, subtree) = match phase {
                    PathVisitPhase::True if negated => {
                        effects.checkpoint(PathWork::FramePush).await?;
                        effects.reserve_frames(&mut frames).await?;
                        frames.push(PathVisitFrame {
                            interior,
                            interior_value,
                            phase: PathVisitPhase::Uncertain { if_true: result },
                            checkpoint: None,
                            new_range: 0..0,
                        });
                        step = PathVisitStep::Impossible;
                        continue;
                    }
                    PathVisitPhase::True => (
                        PathVisitPhase::Uncertain { if_true: result },
                        interior.constraint.when_unconstrained(),
                        interior.if_uncertain,
                    ),
                    PathVisitPhase::Uncertain { if_true } => (
                        PathVisitPhase::False {
                            if_true,
                            if_uncertain: result,
                        },
                        interior.constraint.when_false(),
                        if negated {
                            effects
                                .or_nodes(interior.if_false, interior.if_uncertain)
                                .await?
                        } else {
                            interior.if_false
                        },
                    ),
                    PathVisitPhase::False {
                        if_true,
                        if_uncertain,
                    } => {
                        step = PathVisitStep::Return(
                            effects
                                .leave_interior(
                                    visitor,
                                    &interior_value,
                                    if_true,
                                    if_uncertain,
                                    result,
                                )
                                .await?,
                        );
                        continue;
                    }
                };
                let edge = path_enter_edge_with(path, assignment, effects).await?;
                effects.checkpoint(PathWork::FramePush).await?;
                effects.reserve_frames(&mut frames).await?;
                frames.push(PathVisitFrame {
                    interior,
                    interior_value,
                    phase,
                    checkpoint: Some(edge.checkpoint),
                    new_range: edge.new_range,
                });
                if edge.found_conflict {
                    PathVisitStep::Impossible
                } else {
                    PathVisitStep::Node(subtree)
                }
            }
        };
    }
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_enter_edge_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    assignment: ConstraintAssignment,
    effects: &mut E,
) -> Result<EdgeOutcome, E::Error> {
    effects
        .checkpoint(PathWork::Advance(PathAdvance::Edge))
        .await?;
    // Record the assignments and fuel to restore after this edge. The returned
    // range includes only this edge's new facts, even if its subtree adds more.
    let start = path.assignments.len();
    let checkpoint = EdgeCheckpoint {
        assignments_start: start,
        fuel_undo_start: path.fuel_undo.len(),
        remaining_overall_fuel: path.remaining_overall_fuel,
    };
    // The pending checkpoint stays owned here during discovery, before the
    // traversal has a completed edge to install in its frame vector.
    effects
        .trace_path(
            PathTrace::EnterEdge {
                assignment,
                assignments_start: start,
            },
            path,
        )
        .await?;
    debug_assert!(path.assignment_queue.is_empty());
    effects
        .reserve_path(path, PathReserve::Queue { additional: 1 })
        .await?;
    path.assignment_queue
        .push_back((assignment, AssignmentFuel::origin()));
    let source_constraint = assignment.constraint();
    let found_conflict = path_drain_assignments_with(path, source_constraint, effects)
        .await?
        .is_err();
    if !found_conflict {
        effects
            .trace_path(
                PathTrace::NewAssignments {
                    assignments_start: start,
                },
                path,
            )
            .await?;
    }
    Ok(EdgeOutcome {
        checkpoint,
        new_range: start..path.assignments.len(),
        found_conflict,
    })
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_drain_assignments_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    source_constraint: ConstraintId,
    effects: &mut E,
) -> Result<Result<(), PathAssignmentConflict>, E::Error> {
    loop {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::Queue))
            .await?;
        let Some((assignment, fuel)) = path.assignment_queue.pop_front() else {
            return Ok(Ok(()));
        };
        if let Err(conflict) =
            path_add_assignment_with(path, assignment, source_constraint, fuel, effects).await?
        {
            return Ok(Err(conflict));
        }
    }
}

/// Adds a new assignment, along with any derived information that we can infer from the new
/// assignment combined with the assignments we've already seen. If any of this causes the path
/// to become invalid, due to a contradiction, returns a [`PathAssignmentConflict`] error.
#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_add_assignment_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    assignment: ConstraintAssignment,
    source_constraint: ConstraintId,
    fuel: AssignmentFuel,
    effects: &mut E,
) -> Result<Result<(), PathAssignmentConflict>, E::Error> {
    effects
        .checkpoint(PathWork::Advance(PathAdvance::Assignment))
        .await?;
    if matches!(assignment, ConstraintAssignment::Unconstrained(_)) {
        effects
            .checkpoint(PathWork::Access(PathTable::Assignments))
            .await?;
        if path.contains_constraint(assignment.constraint()) {
            return Ok(Ok(()));
        }
        // Since we don't know whether the constraint holds, we cannot derive
        // additional information from its sequent map. Retain the evidence only.
        effects.reserve_path(path, PathReserve::Assignments).await?;
        path.assignments
            .insert(assignment, (source_constraint, fuel.remaining));
        return Ok(Ok(()));
    }
    if path.assignment_holds(assignment.negated()) {
        effects
            .trace_path(PathTrace::AssignmentConflict { assignment }, path)
            .await?;
        return Ok(Err(PathAssignmentConflict));
    }
    effects
        .checkpoint(PathWork::Access(PathTable::Assignments))
        .await?;
    let existing = path
        .assignments
        .get_full(&assignment)
        .map(|(index, _, value)| (index, *value));
    if let Some((index, (_, existing_fuel))) = existing {
        // Origin provenance takes precedence even when fuel is already sufficient.
        if !fuel.is_derived() {
            path.assignments[index].0 = source_constraint;
        }
        if existing_fuel >= fuel.remaining {
            return Ok(Ok(()));
        }
        // A different derivation chain can replenish this assignment, allowing
        // consequences that previously ran out of path fuel to be considered again.
        effects.reserve_path(path, PathReserve::FuelUndo).await?;
        path.fuel_undo.push((index, existing_fuel));
        path.assignments[index].1 = fuel.remaining;
    } else {
        if let Some(fuel_cost) = fuel.consumed {
            path.remaining_overall_fuel = match path.remaining_overall_fuel.checked_sub(fuel_cost) {
                Some(updated_fuel) => updated_fuel,
                None => return Ok(Ok(())),
            };
        }
        let index = path.assignments.len();
        effects.reserve_path(path, PathReserve::Assignments).await?;
        path.assignments
            .insert(assignment, (source_constraint, fuel.remaining));
        let required = assignment.constraint().as_usize() + 1;
        match assignment {
            ConstraintAssignment::Positive(_) => {
                effects
                    .checkpoint(PathWork::FillIndices {
                        appended: required.saturating_sub(path.positive_assignment_indices.len()),
                    })
                    .await?;
                effects
                    .reserve_path(path, PathReserve::PositiveIndices { required })
                    .await?;
            }
            ConstraintAssignment::Negative(_) => {
                effects
                    .checkpoint(PathWork::FillIndices {
                        appended: required.saturating_sub(path.negative_assignment_indices.len()),
                    })
                    .await?;
                effects
                    .reserve_path(path, PathReserve::NegativeIndices { required })
                    .await?;
            }
            ConstraintAssignment::Unconstrained(_) => {}
        }
        path.record_assignment_index(assignment, index);
    }
    effects
        .checkpoint(PathWork::ClearNewAssignments {
            entries: path.new_assignments.len(),
            reported_capacity: path.new_assignments.capacity(),
        })
        .await?;
    path.new_assignments.clear();
    path_discover_constraint_with(path, assignment.constraint(), effects).await?;
    // TODO: This is deliberately naive while the sequent maps remain small.
    for i in 0..path.sequents.len() {
        let sequent = path.sequents[i];
        if let Err(conflict) = path_check_sequent_with(path, sequent, effects).await? {
            return Ok(Err(conflict));
        }
    }
    effects
        .checkpoint(PathWork::DrainNewAssignments {
            entries: path.new_assignments.len(),
            reported_capacity: path.new_assignments.capacity(),
        })
        .await?;
    let additional = path.new_assignments.len();
    effects
        .reserve_path(path, PathReserve::Queue { additional })
        .await?;
    path.assignment_queue.extend(path.new_assignments.drain(..));
    Ok(Ok(()))
}

/// Update our sequent map to ensure that it holds all of the sequents that involve the given
/// constraint. We do not calculate the new sequents directly. Instead, we call
/// [`SequentMap::for_constraint`] and [`for_constraint_pair`][SequentMap::for_constraint_pair]
/// to calculate _and cache_ the constraints, so that if we walk another constraint set
/// containing this constraint, we reuse the work to calculate its sequents.
#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_discover_constraint_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    constraint: ConstraintId,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects
        .checkpoint(PathWork::Advance(PathAdvance::Discovery))
        .await?;
    effects
        .checkpoint(PathWork::Access(PathTable::Discovered))
        .await?;
    if !path.discovered.contains_key(&constraint) {
        effects.reserve_path(path, PathReserve::Discovered).await?;
    }
    let (constraint_index, existing) = path.discovered.insert_full(constraint, true);
    if existing.is_some_and(|existing| existing) {
        return Ok(());
    }
    let constraint_data = effects.constraint_data(constraint).await?;
    let map = effects.single_sequents(constraint_data).await?;
    let added = path_import_sequents_with(path, map, effects).await?;
    // Source-order replay depends on knowing which sequents were discovered for
    // each constraint. Keep this local view after importing the cached map.

    let mut consequents = Vec::new();
    effects
        .checkpoint(PathWork::ReplayScan {
            entries: added.len(),
        })
        .await?;
    for index in added {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::Replay))
            .await?;
        let sequent = path.sequents[index];
        if let Sequent::SingleImplication { post, .. } | Sequent::PairImplication { post, .. } =
            sequent
        {
            effects.reserve_replay(&mut consequents).await?;
            consequents.push(post);
        }
    }
    effects
        .checkpoint(PathWork::Access(PathTable::SingleReplay))
        .await?;
    if !path.single_replay_consequents.contains_key(&constraint) {
        effects
            .reserve_path(path, PathReserve::SingleReplay)
            .await?;
    }
    path.single_replay_consequents
        .insert(constraint, consequents);

    for existing_index in 0..path.discovered.len() {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::Pair))
            .await?;
        let existing = *path
            .discovered
            .get_index(existing_index)
            .expect("element should be present")
            .0;
        if existing == constraint {
            continue;
        }
        let existing_data = effects.constraint_data(existing).await?;
        // Independent typevars still need disjointness/invalidity checks, but do
        // not otherwise participate in sequent discovery.
        if effects
            .independent_pair_skip(existing, constraint, &path.independent_typevars)
            .await?
        {
            continue;
        }
        if effects
            .pair_cannot_produce(existing_data, constraint_data)
            .await?
        {
            continue;
        }
        let (a, a_data, b, b_data) = if existing_index < constraint_index {
            (existing, existing_data, constraint, constraint_data)
        } else {
            (constraint, constraint_data, existing, existing_data)
        };
        effects
            .checkpoint(PathWork::Access(PathTable::ElaboratedPairs))
            .await?;
        if path.elaborated_pairs.contains(&(a, b)) {
            continue;
        }
        effects
            .reserve_path(path, PathReserve::ElaboratedPairs)
            .await?;
        path.elaborated_pairs.insert((a, b));
        let map = effects.pair_sequents(a_data, b_data).await?;
        let added = path_import_sequents_with(path, map, effects).await?;

        let mut consequents = Vec::new();
        effects
            .checkpoint(PathWork::ReplayScan {
                entries: added.len(),
            })
            .await?;
        for index in added {
            effects
                .checkpoint(PathWork::Advance(PathAdvance::Replay))
                .await?;
            let sequent = path.sequents[index];
            if let Sequent::SingleImplication { post, .. } | Sequent::PairImplication { post, .. } =
                sequent
            {
                effects.reserve_replay(&mut consequents).await?;
                consequents.push(post);
            }
        }
        effects
            .checkpoint(PathWork::Access(PathTable::PairReplay))
            .await?;
        if !path.pair_replay_consequents.contains_key(&(a, b)) {
            effects.reserve_path(path, PathReserve::PairReplay).await?;
        }
        path.pair_replay_consequents.insert((a, b), consequents);
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_import_sequents_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    map: &SequentMap<'db>,
    effects: &mut E,
) -> Result<Range<usize>, E::Error> {
    let start = path.sequents.len();
    for group in &map.sequents {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::ImportGroup))
            .await?;
        match group {
            SequentGroup::Ungrouped(sequents) => {
                path_import_slice_with(path, sequents, effects).await?;
            }
            SequentGroup::Grouped {
                equivalence,
                leftwards,
                rightwards,
            } => {
                let left_first = effects.group_imports_left_first(*equivalence).await?;
                let (first, second) = if left_first {
                    (leftwards, rightwards)
                } else {
                    (rightwards, leftwards)
                };
                path_import_slice_with(path, first, effects).await?;
                path_import_slice_with(path, second, effects).await?;
            }
        }
    }
    Ok(start..path.sequents.len())
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_import_slice_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    sequents: &[Sequent<Constraint<'db>>],
    effects: &mut E,
) -> Result<(), E::Error> {
    for sequent in sequents {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::ImportSequent))
            .await?;
        let sequent = path_import_sequent_with(*sequent, effects).await?;
        effects.reserve_path(path, PathReserve::Sequents).await?;
        path.sequents.push(sequent);
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_import_sequent_with<'db, E: PathEffects<'db>>(
    sequent: Sequent<Constraint<'db>>,
    effects: &mut E,
) -> Result<Sequent<ConstraintId, u16>, E::Error> {
    Ok(match sequent {
        Sequent::SingleTautology { ante } => {
            let ante = effects.intern_constraint(ante).await?;
            Sequent::SingleTautology { ante }
        }
        Sequent::PairImpossibility { ante1, ante2 } => {
            let ante1 = effects.intern_constraint(ante1).await?;
            let ante2 = effects.intern_constraint(ante2).await?;
            Sequent::PairImpossibility { ante1, ante2 }
        }
        Sequent::TripleImpossibility {
            ante1,
            ante2,
            ante3,
        } => {
            let ante1 = effects.intern_constraint(ante1).await?;
            let ante2 = effects.intern_constraint(ante2).await?;
            let ante3 = effects.intern_constraint(ante3).await?;
            Sequent::TripleImpossibility {
                ante1,
                ante2,
                ante3,
            }
        }
        Sequent::SingleImplication { ante, post, .. } => {
            let ante = effects.intern_constraint(ante).await?;
            let post = effects.intern_constraint(post).await?;
            let (ante_depth, _) = effects.constraint_depth(ante).await?;
            let (post_constructor_depth, post_typevar_depth) =
                effects.constraint_depth(post).await?;
            let fuel_cost = sequent_fuel_cost_from_depths(
                post_constructor_depth,
                post_typevar_depth,
                ante_depth,
            );
            Sequent::SingleImplication {
                ante,
                post,
                fuel_cost,
            }
        }
        Sequent::PairImplication {
            ante1, ante2, post, ..
        } => {
            let ante1 = effects.intern_constraint(ante1).await?;
            let ante2 = effects.intern_constraint(ante2).await?;
            let post = effects.intern_constraint(post).await?;
            let (ante1_depth, _) = effects.constraint_depth(ante1).await?;
            let (ante2_depth, _) = effects.constraint_depth(ante2).await?;
            let (post_constructor_depth, post_typevar_depth) =
                effects.constraint_depth(post).await?;
            let fuel_cost = sequent_fuel_cost_from_depths(
                post_constructor_depth,
                post_typevar_depth,
                ante1_depth.max(ante2_depth),
            );
            Sequent::PairImplication {
                ante1,
                ante2,
                post,
                fuel_cost,
            }
        }
    })
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_check_sequent_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    sequent: Sequent<ConstraintId, u16>,
    effects: &mut E,
) -> Result<Result<(), PathAssignmentConflict>, E::Error> {
    effects
        .checkpoint(PathWork::Advance(PathAdvance::SequentCheck))
        .await?;
    match sequent {
        Sequent::SingleTautology { ante } => {
            // The rule says ante is always true, but this path asserts its negation.
            if path.assignment_holds(ante.when_false()) {
                effects
                    .trace_path(PathTrace::SingleConflict { ante }, path)
                    .await?;
                return Ok(Err(PathAssignmentConflict));
            }
        }
        Sequent::PairImpossibility { ante1, ante2 } => {
            // This pair is impossible, but the path asserts both constraints.
            if path.assignment_holds(ante1.when_true()) && path.assignment_holds(ante2.when_true())
            {
                effects
                    .trace_path(PathTrace::PairConflict { ante1, ante2 }, path)
                    .await?;
                return Ok(Err(PathAssignmentConflict));
            }
        }
        Sequent::TripleImpossibility {
            ante1,
            ante2,
            ante3,
        } => {
            // All three constraints hold on the path despite the impossibility rule.
            if path.assignment_holds(ante1.when_true())
                && path.assignment_holds(ante2.when_true())
                && path.assignment_holds(ante3.when_true())
            {
                effects
                    .trace_path(
                        PathTrace::TripleConflict {
                            ante1,
                            ante2,
                            ante3,
                        },
                        path,
                    )
                    .await?;
                return Ok(Err(PathAssignmentConflict));
            }
        }
        Sequent::SingleImplication {
            ante,
            post,
            fuel_cost,
        } => {
            path_check_single_implication_with(path, ante, post, fuel_cost, effects).await?;
        }
        Sequent::PairImplication {
            ante1,
            ante2,
            post,
            fuel_cost,
        } => {
            path_check_pair_implication_with(path, ante1, ante2, post, fuel_cost, effects).await?;
        }
    }
    Ok(Ok(()))
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_check_single_implication_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    ante: ConstraintId,
    post: ConstraintId,
    fuel_cost: u16,
    effects: &mut E,
) -> Result<(), E::Error> {
    let constraint = effects.constraint_data(post).await?;
    if effects.reflexive_constraint(constraint).await? {
        return Ok(());
    }
    let Some(available_fuel) = path.max_remaining_fuel_for(ante.when_true()) else {
        return Ok(());
    };
    if let Some(post_fuel) = available_fuel.checked_sub(fuel_cost) {
        let assignment = post.when_true();
        effects
            .checkpoint(PathWork::Access(PathTable::NewAssignments))
            .await?;
        if !path.new_assignments.contains_key(&assignment) {
            effects
                .reserve_path(path, PathReserve::NewAssignments)
                .await?;
        }
        path.enqueue_assignment(assignment, AssignmentFuel::derived(fuel_cost, post_fuel));
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_satisfaction]
async fn path_check_pair_implication_with<'db, E: PathEffects<'db>>(
    path: &mut PathAssignments,
    ante1: ConstraintId,
    ante2: ConstraintId,
    post: ConstraintId,
    fuel_cost: u16,
    effects: &mut E,
) -> Result<(), E::Error> {
    let constraint = effects.constraint_data(post).await?;
    if effects.reflexive_constraint(constraint).await? {
        return Ok(());
    }
    let Some(ante1_fuel) = path.max_remaining_fuel_for(ante1.when_true()) else {
        return Ok(());
    };
    let Some(ante2_fuel) = path.max_remaining_fuel_for(ante2.when_true()) else {
        return Ok(());
    };
    let available_fuel = ante1_fuel.min(ante2_fuel);
    if let Some(post_fuel) = available_fuel.checked_sub(fuel_cost) {
        let assignment = post.when_true();
        effects
            .checkpoint(PathWork::Access(PathTable::NewAssignments))
            .await?;
        if !path.new_assignments.contains_key(&assignment) {
            effects
                .reserve_path(path, PathReserve::NewAssignments)
                .await?;
        }
        path.enqueue_assignment(assignment, AssignmentFuel::derived(fuel_cost, post_fuel));
    }
    Ok(())
}

impl<'db> SyncPathEffects<'db> for OrdinarySatisfaction<'_, '_, 'db> {
    type Error = Infallible;

    fn checkpoint(&mut self, work: PathWork) -> Result<(), Infallible> {
        unrestricted(admit_path_work(work, &mut Unrestricted));
        Ok(())
    }
    fn interior_data(&mut self, node: NodeId) -> Result<InteriorNodeData, Infallible> {
        Ok(self.storage.interior_node_data(node))
    }
    fn constraint_data(&mut self, id: ConstraintId) -> Result<Constraint<'db>, Infallible> {
        Ok(self.storage.constraint_data(id))
    }
    fn single_sequents(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<&'db SequentMap<'db>, Infallible> {
        Ok(SequentMap::for_constraint(self.db, self.env, constraint))
    }
    fn pair_sequents(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> Result<&'db SequentMap<'db>, Infallible> {
        Ok(SequentMap::for_constraint_pair(
            self.db, self.env, left, right,
        ))
    }
    fn pair_cannot_produce(
        &mut self,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> Result<bool, Infallible> {
        Ok(SequentMap::pair_cannot_produce_sequents(
            self.db, self.env, left, right,
        ))
    }
    fn independent_pair_skip(
        &mut self,
        existing: ConstraintId,
        current: ConstraintId,
        independent: &FxHashSet<TypeVarId>,
    ) -> Result<bool, Infallible> {
        Ok(unrestricted(independent_pair_skip_with(
            self.storage,
            existing,
            current,
            independent,
            &mut Unrestricted,
        )))
    }
    fn group_imports_left_first(
        &mut self,
        equivalence: TypeVarEquivalenceBound<'db>,
    ) -> Result<bool, Infallible> {
        let (first, _) = equivalence.in_builder(self.db, self.storage);
        Ok(first.is_same_typevar_as(self.db, equivalence.left))
    }
    fn intern_constraint(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<ConstraintId, Infallible> {
        Ok(self
            .storage
            .intern_constraint(self.db, self.env, constraint))
    }
    fn constraint_depth(&mut self, id: ConstraintId) -> Result<(u16, u16), Infallible> {
        Ok(self
            .storage
            .cached_constraint_bound_depth(self.db, self.env, id))
    }
    fn reflexive_constraint(&mut self, constraint: Constraint<'db>) -> Result<bool, Infallible> {
        Ok(constraint.is_reflexive_typevar_relation(self.db))
    }
    fn trace_path(&mut self, event: PathTrace, path: &PathAssignments) -> Result<(), Infallible> {
        let db = self.db;
        let env = self.env;
        let storage = &*self.storage;
        match event {
            PathTrace::EnterEdge {
                assignment,
                assignments_start: start,
            } => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    before = %format_args!(
                        "[{}]",
                        path.assignments[..start].iter().map(|(assignment, _)| {
                            assignment.display(db, env, storage)
                        }).format(", "),
                    ),
                    edge = %assignment.display(db, env, storage),
                    "walk edge",
                );
            }
            PathTrace::NewAssignments {
                assignments_start: start,
            } => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    new = %format_args!(
                        "[{}]",
                        path.assignments[start..].iter().map(|(assignment, _)| {
                            assignment.display(db, env, storage)
                        }).format(", "),
                    ),
                    "new assignments",
                );
            }
            PathTrace::AssignmentConflict { assignment } => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    assignment = %assignment.display(db, env, storage),
                    facts = %format_args!(
                        "[{}]",
                        path.assignments.iter().map(|(assignment, _)| {
                            assignment.display(db, env, storage)
                        }).format(", "),
                    ),
                    "found contradiction",
                );
            }
            PathTrace::SingleConflict { ante } => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante = %ante.display(db, env, storage),
                    facts = %format_args!(
                        "[{}]",
                        path.assignments.iter().map(|(assignment, _)| {
                            assignment.display(db, env, storage)
                        }).format(", "),
                    ),
                    "found contradiction",
                );
            }
            PathTrace::PairConflict { ante1, ante2 } => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante1 = %ante1.display(db, env, storage),
                    ante2 = %ante2.display(db, env, storage),
                    facts = %format_args!(
                        "[{}]",
                        path.assignments.iter().map(|(assignment, _)| {
                            assignment.display(db, env, storage)
                        }).format(", "),
                    ),
                    "found contradiction",
                );
            }
            PathTrace::TripleConflict {
                ante1,
                ante2,
                ante3,
            } => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante1 = %ante1.display(db, env, storage),
                    ante2 = %ante2.display(db, env, storage),
                    ante3 = %ante3.display(db, env, storage),
                    facts = %format_args!(
                        "[{}]",
                        path.assignments.iter().map(|(assignment, _)| {
                            assignment.display(db, env, storage)
                        }).format(", "),
                    ),
                    "found contradiction",
                );
            }
        }
        Ok(())
    }
    fn reserve_path(
        &mut self,
        path: &mut PathAssignments,
        request: PathReserve,
    ) -> Result<(), Infallible> {
        unrestricted(reserve_path_with(path, request, &mut Unrestricted));
        Ok(())
    }
    fn reserve_replay(&mut self, ids: &mut Vec<ConstraintId>) -> Result<(), Infallible> {
        unrestricted(reserve_path_replay_with(ids, &mut Unrestricted));
        Ok(())
    }
}

impl<'db, V: PathVisitor> SyncPathVisitEffects<'db, V> for OrdinarySatisfaction<'_, '_, 'db> {
    fn visit_node(&mut self, visitor: &mut V) -> Result<ControlFlow<V::Break>, Infallible> {
        Ok(visitor.visit_node())
    }

    fn visit_satisfied(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Infallible> {
        Ok(visitor.visit_satisfied(self.db, self.storage, path))
    }

    fn visit_unsatisfied(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Infallible> {
        Ok(visitor.visit_unsatisfied(self.db, self.storage, path))
    }

    fn visit_impossible(
        &mut self,
        visitor: &mut V,
        path: &PathAssignments,
    ) -> Result<ControlFlow<V::Break, V::Result>, Infallible> {
        Ok(visitor.visit_impossible(self.db, self.storage, path))
    }
    fn enter_interior(
        &mut self,
        visitor: &mut V,
        interior: InteriorNode,
    ) -> Result<ControlFlow<V::Break, V::Interior>, Infallible> {
        Ok(visitor.enter_interior(self.db, self.storage, interior))
    }
    fn visit_edge(
        &mut self,
        visitor: &mut V,
        interior_value: &V::Interior,
        subtree: V::Result,
        path: &PathAssignments,
        new_range: Range<usize>,
    ) -> Result<ControlFlow<V::Break, V::Result>, Infallible> {
        Ok(visitor.visit_edge(
            self.db,
            self.storage,
            interior_value,
            subtree,
            path,
            new_range,
        ))
    }
    fn leave_interior(
        &mut self,
        visitor: &mut V,
        interior_value: &V::Interior,
        if_true: V::Result,
        if_uncertain: V::Result,
        if_false: V::Result,
    ) -> Result<ControlFlow<V::Break, V::Result>, Infallible> {
        Ok(visitor.leave_interior(
            self.db,
            self.storage,
            interior_value,
            if_true,
            if_uncertain,
            if_false,
        ))
    }
    fn or_nodes(&mut self, left: NodeId, right: NodeId) -> Result<NodeId, Infallible> {
        Ok(left.or(self.storage, right))
    }
    fn reserve_frames(&mut self, frames: &mut Vec<PathVisitFrame<V>>) -> Result<(), Infallible> {
        unrestricted(reserve_path_frames_with(frames, &mut Unrestricted));
        Ok(())
    }
}

pub(super) fn reserve_path_with<C: TddControl>(
    path: &mut PathAssignments,
    request: PathReserve,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    match request {
        PathReserve::Sequents => {
            reserve_vec(&mut path.sequents, 1, AllocationKind::PathSequents, control)?
        }
        PathReserve::FuelUndo => reserve_vec(
            &mut path.fuel_undo,
            1,
            AllocationKind::PathFuelUndo,
            control,
        )?,
        PathReserve::PositiveIndices { required } => {
            let additional = required.saturating_sub(path.positive_assignment_indices.len());
            reserve_vec(
                &mut path.positive_assignment_indices.raw,
                additional,
                AllocationKind::PathPositiveIndices,
                control,
            )?;
        }
        PathReserve::NegativeIndices { required } => {
            let additional = required.saturating_sub(path.negative_assignment_indices.len());
            reserve_vec(
                &mut path.negative_assignment_indices.raw,
                additional,
                AllocationKind::PathNegativeIndices,
                control,
            )?;
        }
        PathReserve::Queue { additional } => {
            let required = path
                .assignment_queue
                .len()
                .checked_add(additional)
                .ok_or(TddError::CapacityExhausted)?;
            if required > path.assignment_queue.capacity() {
                let mut plan = sequence_growth::<(ConstraintAssignment, AssignmentFuel), C::Error>(
                    path.assignment_queue.capacity(),
                    required,
                )?;
                plan.relocation_units = path.assignment_queue.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathQueue,
                    plan,
                })?;
                path.assignment_queue
                    .reserve_exact(plan.requested_capacity - path.assignment_queue.len());
            }
        }

        PathReserve::Assignments => {
            if path.assignments.len() == path.assignments.capacity() {
                let required = path
                    .assignments
                    .len()
                    .checked_add(1)
                    .ok_or(TddError::CapacityExhausted)?;
                let mut plan = sequence_growth::<
                    (ConstraintAssignment, (ConstraintId, u16)),
                    C::Error,
                >(path.assignments.capacity(), required)?;
                plan.relocation_units = path.assignments.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathAssignments,
                    plan,
                })?;
                path.assignments
                    .reserve(plan.requested_capacity - path.assignments.len());
            }
        }

        PathReserve::Discovered => {
            if path.discovered.len() == path.discovered.capacity() {
                let required = path
                    .discovered
                    .len()
                    .checked_add(1)
                    .ok_or(TddError::CapacityExhausted)?;
                let mut plan = sequence_growth::<(ConstraintId, bool), C::Error>(
                    path.discovered.capacity(),
                    required,
                )?;
                plan.relocation_units = path.discovered.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathDiscovered,
                    plan,
                })?;
                path.discovered
                    .reserve(plan.requested_capacity - path.discovered.len());
            }
        }

        PathReserve::ElaboratedPairs => {
            if path.elaborated_pairs.len() == path.elaborated_pairs.capacity() {
                let required = path
                    .elaborated_pairs
                    .len()
                    .checked_add(1)
                    .ok_or(TddError::CapacityExhausted)?;
                let mut plan = sequence_growth::<(ConstraintId, ConstraintId), C::Error>(
                    path.elaborated_pairs.capacity(),
                    required,
                )?;
                plan.relocation_units = path.elaborated_pairs.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathElaboratedPairs,
                    plan,
                })?;
                path.elaborated_pairs
                    .reserve(plan.requested_capacity - path.elaborated_pairs.len());
            }
        }

        PathReserve::SingleReplay => {
            if path.single_replay_consequents.len() == path.single_replay_consequents.capacity() {
                let required = path
                    .single_replay_consequents
                    .len()
                    .checked_add(1)
                    .ok_or(TddError::CapacityExhausted)?;
                let mut plan = sequence_growth::<(ConstraintId, Vec<ConstraintId>), C::Error>(
                    path.single_replay_consequents.capacity(),
                    required,
                )?;
                plan.relocation_units = path.single_replay_consequents.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathSingleReplay,
                    plan,
                })?;
                path.single_replay_consequents
                    .reserve(plan.requested_capacity - path.single_replay_consequents.len());
            }
        }

        PathReserve::PairReplay => {
            if path.pair_replay_consequents.len() == path.pair_replay_consequents.capacity() {
                let required = path
                    .pair_replay_consequents
                    .len()
                    .checked_add(1)
                    .ok_or(TddError::CapacityExhausted)?;
                let mut plan = sequence_growth::<
                    ((ConstraintId, ConstraintId), Vec<ConstraintId>),
                    C::Error,
                >(path.pair_replay_consequents.capacity(), required)?;
                plan.relocation_units = path.pair_replay_consequents.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathPairReplay,
                    plan,
                })?;
                path.pair_replay_consequents
                    .reserve(plan.requested_capacity - path.pair_replay_consequents.len());
            }
        }

        PathReserve::NewAssignments => {
            if path.new_assignments.len() == path.new_assignments.capacity() {
                let required = path
                    .new_assignments
                    .len()
                    .checked_add(1)
                    .ok_or(TddError::CapacityExhausted)?;
                let mut plan = sequence_growth::<(ConstraintAssignment, AssignmentFuel), C::Error>(
                    path.new_assignments.capacity(),
                    required,
                )?;
                plan.relocation_units = path.new_assignments.len();
                control.admit(TddWork::Grow {
                    allocation: AllocationKind::PathNewAssignments,
                    plan,
                })?;
                path.new_assignments
                    .reserve(plan.requested_capacity - path.new_assignments.len());
            }
        }
    }
    Ok(())
}

pub(super) fn reserve_path_frames_with<V: PathVisitor, C: TddControl>(
    frames: &mut Vec<PathVisitFrame<V>>,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    reserve_vec(frames, 1, AllocationKind::PathFrames, control)
}

pub(super) fn reserve_path_replay_with<C: TddControl>(
    ids: &mut Vec<ConstraintId>,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    reserve_vec(ids, 1, AllocationKind::PathReplayIds, control)
}

pub(super) fn reserve_path_typevars_with<C: TddControl>(
    set: &mut FxHashSet<TypeVarId>,
    kind: PathTypevarSet,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    if set.len() == set.capacity() {
        let required = set
            .len()
            .checked_add(1)
            .ok_or(TddError::CapacityExhausted)?;
        let mut plan = sequence_growth::<TypeVarId, C::Error>(set.capacity(), required)?;
        plan.relocation_units = set.len();
        let allocation = match kind {
            PathTypevarSet::Independent => AllocationKind::PathIndependentTypevars,
            PathTypevarSet::Dependent => AllocationKind::PathDependentTypevars,
        };
        control.admit(TddWork::Grow { allocation, plan })?;
        set.reserve(plan.requested_capacity - set.len());
    }
    Ok(())
}

#[derive(Debug)]
struct PathAssignmentConflict;

#[cfg(test)]
mod tests {
    use std::thread;

    use super::super::solutions::SolutionWalker;
    use super::super::variables::ConcreteLowerBound;
    use super::super::*;

    use crate::db::tests::{TestDb, setup_db};
    use crate::types::{BoundTypeVarInstance, KnownClass, TypeVarVariance};
    use ruff_python_ast::name::Name;

    fn create_typevar<'db>(db: &'db TestDb, name: &'static str) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new_static(name),
            TypeVarVariance::Invariant,
        )
    }

    fn create_constraint<'db, 'c>(
        db: &'db TestDb,
        builder: &'c ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        bound: KnownClass,
    ) -> ConstraintSet<'db, 'c> {
        let env = db.program_environment();
        let ty = bound.to_instance(db, &env);
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, bound_typevar, ty)
    }

    #[test]
    fn eager_and_lazy_negation_are_equivalent() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();

        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_bool = create_constraint(db, &builder, t, KnownClass::Bool);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let u_int = create_constraint(db, &builder, u, KnownClass::Int);

        let lhs = t_int.or(db, &builder, || u_str);
        let rhs = t_bool.or(db, &builder, || u_int);
        let intersection = lhs.and(db, &builder, || rhs);
        let tautology = lhs.or(db, &builder, || lhs.negate(db, &builder));

        let t_bool_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Bool.to_instance(db, &env),
        );
        let t_int_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Int.to_instance(db, &env),
        );
        let implication = t_bool_upper
            .negate(db, &builder)
            .or(db, &builder, || t_int_upper);

        for set in [lhs, rhs, intersection, tautology, implication] {
            assert_eq!(
                set.is_always_satisfied(db, &env),
                set.negate(db, &builder).is_never_satisfied(db, &env)
            );
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum PathFoldBreak {
        Satisfied,
        Unsatisfied,
        Impossible,
        Combine,
    }

    /// A path fold that reconstructs a constraint set from its satisfied paths and can abort at
    /// a specified callback.
    struct ReconstructPathFold {
        break_at: Option<PathFoldBreak>,
    }

    impl ReconstructPathFold {
        fn result(
            &self,
            at: PathFoldBreak,
            result: (NodeId, Option<SourceOrderId>),
        ) -> ControlFlow<PathFoldBreak, (NodeId, Option<SourceOrderId>)> {
            if self.break_at == Some(at) {
                ControlFlow::Break(at)
            } else {
                ControlFlow::Continue(result)
            }
        }
    }

    impl PathFold for ReconstructPathFold {
        type Result = (NodeId, Option<SourceOrderId>);
        type Break = PathFoldBreak;

        fn satisfied<'db>(
            &mut self,
            _db: &'db dyn Db,
            storage: &mut ConstraintSetStorage<'db>,
            path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            let result =
                path.assignments
                    .iter()
                    .fold((ALWAYS_TRUE, None), |result, (assignment, _)| {
                        let (node, source_order) = result;
                        let (assignment, assignment_source_order) =
                            Node::new_satisfied_constraint(storage, *assignment);
                        (
                            node.and(storage, assignment),
                            storage.ordered_source_order(source_order, assignment_source_order),
                        )
                    });
            self.result(PathFoldBreak::Satisfied, result)
        }

        fn unsatisfied<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            _path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.result(PathFoldBreak::Unsatisfied, (ALWAYS_FALSE, None))
        }

        fn impossible<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            _path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.result(PathFoldBreak::Impossible, (ALWAYS_FALSE, None))
        }

        fn combine<'db>(
            &mut self,
            _db: &'db dyn Db,
            storage: &mut ConstraintSetStorage<'db>,
            if_true: Self::Result,
            if_uncertain: Self::Result,
            if_false: Self::Result,
        ) -> ControlFlow<Self::Break, Self::Result> {
            let (if_true, if_true_source_order) = if_true;
            let (if_uncertain, if_uncertain_source_order) = if_uncertain;
            let (if_false, if_false_source_order) = if_false;
            let node = if_true.or(storage, if_uncertain).or(storage, if_false);
            let source_order =
                storage.ordered_source_order(if_true_source_order, if_uncertain_source_order);
            let source_order = storage.ordered_source_order(source_order, if_false_source_order);
            self.result(PathFoldBreak::Combine, (node, source_order))
        }
    }

    fn path_assignments_for<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        node: NodeId,
        source_order: Option<SourceOrderId>,
    ) -> PathAssignments {
        let mut storage = builder.storage.borrow_mut();
        match node.node() {
            Node::AlwaysTrue | Node::AlwaysFalse => PathAssignments::new([], FxHashSet::default()),
            Node::Interior(interior) => {
                interior.path_assignments(db, env, &mut storage, source_order)
            }
        }
    }

    #[test]
    fn path_assignments_follow_constraint_source_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);

        // Construct the set in the opposite order from constraint creation. This ensures the
        // initializer follows the sidecar rather than either TDD traversal or constraint IDs.
        let set = u_str.and(db, &builder, || t_int);
        let path = path_assignments_for(db, &env, &builder, set.node, set.source_order);
        let storage = builder.storage.borrow();
        let expected =
            [u_str.node, t_int.node].map(|node| storage.interior_node_data(node).constraint);
        let actual: Vec<_> = path.discovered.keys().copied().collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn path_fold_reconstructs_constraint_sets() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let v = create_typevar(db, "V");
        let builder = ConstraintSetBuilder::new();

        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_str = create_constraint(db, &builder, t, KnownClass::Str);
        let u_int = create_constraint(db, &builder, u, KnownClass::Int);
        let v_bytes = create_constraint(db, &builder, v, KnownClass::Bytes);
        let union = t_int.or(db, &builder, || u_int);
        let intersection = union.and(db, &builder, || t_str.or(db, &builder, || v_bytes));
        let contradiction = t_int.and(db, &builder, || t_str);
        let tautology = union.or(db, &builder, || union.negate(db, &builder));

        let t_u =
            ConstraintSet::constrain_typevar_upper_bound(db, &env, &builder, t, Type::TypeVar(u));
        let u_int_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            u,
            KnownClass::Int.to_instance(db, &env),
        );
        let int_t = ConstraintSet::constrain_typevar_lower_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Int.to_instance(db, &env),
        );
        let transitive = t_u
            .and(db, &builder, || u_int_upper)
            .and(db, &builder, || int_t)
            .or(db, &builder, || v_bytes);

        for set in [
            ConstraintSet::always(&builder),
            ConstraintSet::never(&builder),
            union,
            intersection,
            contradiction,
            tautology,
            transitive,
        ] {
            let mut path = path_assignments_for(db, &env, &builder, set.node, set.source_order);
            let mut fold = ReconstructPathFold { break_at: None };
            let mut storage = builder.storage.borrow_mut();
            let ControlFlow::Continue((reconstructed, reconstructed_source_order)) =
                path.visit(db, &env, &mut storage, set.node, &mut fold)
            else {
                panic!("reconstruction unexpectedly aborted");
            };
            drop(storage);
            let reconstructed =
                ConstraintSet::from_node(&builder, reconstructed, reconstructed_source_order);
            assert!(
                set.iff(db, &builder, reconstructed)
                    .is_always_satisfied(db, &env)
            );
        }
    }

    #[test]
    fn path_fold_break_restores_path_assignments() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_str = create_constraint(db, &builder, t, KnownClass::Str);
        let u_int = create_constraint(db, &builder, u, KnownClass::Int);
        let set = t_int.and(db, &builder, || t_str).or(db, &builder, || u_int);

        for break_at in [
            PathFoldBreak::Satisfied,
            PathFoldBreak::Unsatisfied,
            PathFoldBreak::Impossible,
            PathFoldBreak::Combine,
        ] {
            let mut path = path_assignments_for(db, &env, &builder, set.node, set.source_order);
            let mut aborting_fold = ReconstructPathFold {
                break_at: Some(break_at),
            };
            let mut storage = builder.storage.borrow_mut();
            assert_eq!(
                path.visit(db, &env, &mut storage, set.node, &mut aborting_fold),
                ControlFlow::Break(break_at)
            );

            let mut completing_fold = ReconstructPathFold { break_at: None };
            let ControlFlow::Continue((reconstructed, reconstructed_source_order)) =
                path.visit(db, &env, &mut storage, set.node, &mut completing_fold)
            else {
                panic!("reconstruction unexpectedly aborted after {break_at:?}");
            };
            drop(storage);
            let reconstructed =
                ConstraintSet::from_node(&builder, reconstructed, reconstructed_source_order);
            assert!(
                set.iff(db, &builder, reconstructed)
                    .is_always_satisfied(db, &env)
            );
        }
    }

    #[test]
    fn solution_walker_break_restores_path_assignments() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_str = create_constraint(db, &builder, t, KnownClass::Str);
        let set = t_int.or(db, &builder, || t_str);
        let source_orders = builder
            .storage
            .borrow()
            .calculate_source_orders(set.source_order);
        let expected = CandidateSolutions::compute(
            db,
            &env,
            &mut builder.storage.borrow_mut(),
            set.node,
            TypeVarSet::from_typevars(db, [t]),
            set.source_order,
        );

        // Both limits interrupt an edge with path-local assignments: the visit limit stops
        // below the root, and the path limit stops after collecting the first alternative.
        for (remaining_paths, remaining_visits, error) in [
            (usize::MAX, 1, ProjectionError::TraversalBudgetExceeded),
            (1, usize::MAX, ProjectionError::PathBudgetExceeded),
        ] {
            let mut path = path_assignments_for(db, &env, &builder, set.node, set.source_order);
            let mut storage = builder.storage.borrow_mut();
            let mut limits = BoundedSolutionLimits {
                remaining_paths,
                remaining_visits,
            };
            let mut walker = SolutionWalker::new(source_orders.clone());
            assert_eq!(
                walker.visit_node(db, &env, &mut storage, &mut path, set.node, &mut limits),
                ControlFlow::Break(error)
            );
            drop(walker);

            let mut limits = UnboundedSolutionLimits;
            let mut walker = SolutionWalker::new(source_orders.clone());
            let ControlFlow::Continue(()) =
                walker.visit_node(db, &env, &mut storage, &mut path, set.node, &mut limits);
            assert_eq!(walker.finish(db, &env, &mut storage), expected);
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum TraversalEvent {
        Node,
        Enter(ConstraintId),
        Leaf(&'static str, Vec<(ConstraintAssignment, u16)>, u16),
        Edge(
            ConstraintId,
            usize,
            Range<usize>,
            Vec<(ConstraintAssignment, u16)>,
            u16,
        ),
        Leave(ConstraintId, [usize; 3]),
    }

    #[derive(Default)]
    struct RecordTraversal {
        events: Vec<TraversalEvent>,
        break_at: Option<usize>,
    }

    impl RecordTraversal {
        fn record(&mut self, event: TraversalEvent) -> ControlFlow<usize, usize> {
            let index = self.events.len();
            self.events.push(event);
            if self.break_at == Some(index) {
                ControlFlow::Break(index)
            } else {
                ControlFlow::Continue(index)
            }
        }

        fn assignments(path: &PathAssignments) -> Vec<(ConstraintAssignment, u16)> {
            path.assignments
                .iter()
                .map(|(assignment, (_, fuel))| (*assignment, *fuel))
                .collect()
        }

        fn leaf(
            &mut self,
            kind: &'static str,
            path: &PathAssignments,
        ) -> ControlFlow<usize, usize> {
            self.record(TraversalEvent::Leaf(
                kind,
                Self::assignments(path),
                path.remaining_overall_fuel,
            ))
        }
    }

    impl PathVisitor for RecordTraversal {
        type Result = usize;
        type Interior = ConstraintId;
        type Break = usize;

        fn visit_node(&mut self) -> ControlFlow<Self::Break> {
            self.record(TraversalEvent::Node).map_continue(|_| ())
        }

        fn enter_interior<'db>(
            &mut self,
            _db: &'db dyn Db,
            storage: &mut ConstraintSetStorage<'db>,
            interior: InteriorNode,
        ) -> ControlFlow<Self::Break, Self::Interior> {
            let constraint = storage.interior_node_data(interior.node()).constraint;
            self.record(TraversalEvent::Enter(constraint))
                .map_continue(|_| constraint)
        }

        fn visit_satisfied<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.leaf("satisfied", path)
        }

        fn visit_unsatisfied<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.leaf("unsatisfied", path)
        }

        fn visit_impossible<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.leaf("impossible", path)
        }

        fn visit_edge<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            interior: &Self::Interior,
            subtree: Self::Result,
            path: &PathAssignments,
            new_range: Range<usize>,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.record(TraversalEvent::Edge(
                *interior,
                subtree,
                new_range,
                Self::assignments(path),
                path.remaining_overall_fuel,
            ))
        }

        fn leave_interior<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            interior: &Self::Interior,
            if_true: Self::Result,
            if_uncertain: Self::Result,
            if_false: Self::Result,
        ) -> ControlFlow<Self::Break, Self::Result> {
            self.record(TraversalEvent::Leave(
                *interior,
                [if_true, if_uncertain, if_false],
            ))
        }
    }

    #[test]
    fn path_traversal_callbacks_restore_assignments_and_fuel() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let b_set = create_constraint(db, &builder, create_typevar(db, "B"), KnownClass::Int);
        let a_set = create_constraint(db, &builder, create_typevar(db, "A"), KnownClass::Int);
        let c_set = create_constraint(db, &builder, create_typevar(db, "C"), KnownClass::Int);
        let mut storage = builder.storage.borrow_mut();
        let [a, b, c] = [a_set.node, b_set.node, c_set.node]
            .map(|node| storage.interior_node_data(node).constraint);
        let root = NodeId::with_uncertain(&mut storage, a, b_set.node, ALWAYS_FALSE, ALWAYS_FALSE);
        let budget = super::OVERALL_FUEL_BUDGET;
        let path_budget = super::PATH_FUEL_BUDGET;
        let baseline = vec![(b.when_true(), 0)];
        let parent = vec![(b.when_true(), 0), (a.when_true(), path_budget)];
        let replenished = vec![
            (b.when_true(), path_budget),
            (a.when_true(), path_budget),
            (c.when_true(), path_budget - 1),
        ];
        let uncertain = vec![(b.when_true(), 0), (a.when_unconstrained(), path_budget)];
        let negative = vec![(b.when_true(), 0), (a.when_false(), path_budget)];

        for negated in [false, true] {
            let expected = if negated {
                vec![
                    TraversalEvent::Node,
                    TraversalEvent::Enter(a),
                    TraversalEvent::Node,
                    TraversalEvent::Enter(b),
                    TraversalEvent::Node,
                    TraversalEvent::Leaf("unsatisfied", replenished.clone(), budget - 1),
                    TraversalEvent::Edge(b, 5, 2..3, replenished.clone(), budget - 1),
                    TraversalEvent::Leaf("impossible", parent.clone(), budget),
                    TraversalEvent::Edge(b, 7, 0..0, parent.clone(), budget),
                    TraversalEvent::Leaf("impossible", parent.clone(), budget),
                    TraversalEvent::Edge(b, 9, 2..2, parent.clone(), budget),
                    TraversalEvent::Leave(b, [6, 8, 10]),
                    TraversalEvent::Edge(a, 11, 1..2, parent.clone(), budget),
                    TraversalEvent::Leaf("impossible", baseline.clone(), budget),
                    TraversalEvent::Edge(a, 13, 0..0, baseline.clone(), budget),
                    TraversalEvent::Node,
                    TraversalEvent::Leaf("satisfied", negative.clone(), budget),
                    TraversalEvent::Edge(a, 16, 1..2, negative.clone(), budget),
                    TraversalEvent::Leave(a, [12, 14, 17]),
                ]
            } else {
                vec![
                    TraversalEvent::Node,
                    TraversalEvent::Enter(a),
                    TraversalEvent::Node,
                    TraversalEvent::Enter(b),
                    TraversalEvent::Node,
                    TraversalEvent::Leaf("satisfied", replenished.clone(), budget - 1),
                    TraversalEvent::Edge(b, 5, 2..3, replenished.clone(), budget - 1),
                    TraversalEvent::Node,
                    TraversalEvent::Leaf("unsatisfied", parent.clone(), budget),
                    TraversalEvent::Edge(b, 8, 2..2, parent.clone(), budget),
                    TraversalEvent::Leaf("impossible", parent.clone(), budget),
                    TraversalEvent::Edge(b, 10, 2..2, parent.clone(), budget),
                    TraversalEvent::Leave(b, [6, 9, 11]),
                    TraversalEvent::Edge(a, 12, 1..2, parent.clone(), budget),
                    TraversalEvent::Node,
                    TraversalEvent::Leaf("unsatisfied", uncertain.clone(), budget),
                    TraversalEvent::Edge(a, 15, 1..2, uncertain.clone(), budget),
                    TraversalEvent::Node,
                    TraversalEvent::Leaf("unsatisfied", negative.clone(), budget),
                    TraversalEvent::Edge(a, 18, 1..2, negative.clone(), budget),
                    TraversalEvent::Leave(a, [13, 16, 19]),
                ]
            };

            for break_at in std::iter::once(None).chain((0..expected.len()).map(Some)) {
                let mut path = PathAssignments::new([a, b, c], FxHashSet::default());
                path.discovered
                    .values_mut()
                    .for_each(|processed| *processed = true);
                path.assignments.insert(b.when_true(), (b, 0));
                path.record_assignment_index(b.when_true(), 0);
                // Entering B replenishes its existing assignment, which then derives C.
                // These explicit sequents isolate traversal rollback from sequent discovery.
                path.sequents.push(super::Sequent::SingleImplication {
                    ante: b,
                    post: c,
                    fuel_cost: 1,
                });
                let assignments_before = path.assignments.clone();
                let mut visitor = RecordTraversal {
                    break_at,
                    ..Default::default()
                };
                let result = if negated {
                    path.visit_negated(db, &env, &mut storage, root, &mut visitor)
                } else {
                    path.visit(db, &env, &mut storage, root, &mut visitor)
                };
                if let Some(index) = break_at {
                    assert_eq!(result, ControlFlow::Break(index));
                    assert_eq!(visitor.events, expected[..=index]);
                } else {
                    assert_eq!(result, ControlFlow::Continue(expected.len() - 1));
                    assert_eq!(visitor.events, expected);
                }
                assert_eq!(path.assignments, assignments_before);
                assert_eq!(path.assignment_index(b.when_true()), Some(0));
                for assignment in [
                    a.when_true(),
                    a.when_false(),
                    b.when_false(),
                    c.when_true(),
                    c.when_false(),
                ] {
                    assert_eq!(path.assignment_index(assignment), None);
                }
                assert!(path.fuel_undo.is_empty());
                assert!(path.assignment_queue.is_empty());
                assert_eq!(path.remaining_overall_fuel, budget);
            }
        }
    }

    struct CountSatisfiedPaths;

    impl PathFold for CountSatisfiedPaths {
        type Result = usize;
        type Break = std::convert::Infallible;

        fn satisfied<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            _path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            ControlFlow::Continue(1)
        }

        fn unsatisfied<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            _path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            ControlFlow::Continue(0)
        }

        fn impossible<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            _path: &PathAssignments,
        ) -> ControlFlow<Self::Break, Self::Result> {
            ControlFlow::Continue(0)
        }

        fn combine<'db>(
            &mut self,
            _db: &'db dyn Db,
            _storage: &mut ConstraintSetStorage<'db>,
            if_true: Self::Result,
            if_uncertain: Self::Result,
            if_false: Self::Result,
        ) -> ControlFlow<Self::Break, Self::Result> {
            ControlFlow::Continue(if_true + if_uncertain + if_false)
        }
    }

    #[test]
    fn deep_path_traversal_and_break_restore_without_recursive_frames() {
        thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let db = setup_db();
                let env = db.program_environment();
                let t = create_typevar(&db, "T");
                let mut storage = ConstraintSetStorage::default();
                let mut path = PathAssignments::new([], FxHashSet::default());
                let mut node = ALWAYS_TRUE;
                for index in 0..16_384 {
                    let constraint = ConcreteLowerBound::new(
                        ConstraintProvenance::Evidence,
                        t,
                        Type::int_literal(index),
                    );
                    let constraint = storage.intern_constraint(&db, &env, constraint.into());
                    // Prepare discovery so this test isolates traversal depth from semantic
                    // operations in sequent derivation, which have their own recursion paths.
                    path.discovered.insert(constraint, true);
                    node = NodeId::new(&mut storage, constraint, node, ALWAYS_FALSE);
                }
                assert_eq!(
                    path.visit(&db, &env, &mut storage, node, &mut CountSatisfiedPaths),
                    ControlFlow::Continue(1)
                );
                assert_eq!(
                    path.visit(&db, &env, &mut storage, node, &mut IsNeverSatisfiedVisitor),
                    ControlFlow::Break(())
                );
                assert!(path.assignments.is_empty());
                assert!(path.fuel_undo.is_empty());
                assert_eq!(path.remaining_overall_fuel, super::OVERALL_FUEL_BUDGET);
            })
            .expect("spawn small-stack path traversal")
            .join()
            .expect("complete small-stack path traversal");
    }
}
