//! [`PathAssignments`] and friends

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::ops::{ControlFlow, Range};

use indexmap::map::Entry;
use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::types::constraints::sequents::{Sequent, SequentMap};
use crate::types::constraints::{
    ConstraintAssignment, ConstraintId, ConstraintSetStorage, Node, NodeId, PathVisitor, TypeVarId,
};
use crate::{Db, FxIndexMap, ProgramEnvironment};

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
    sequents: Vec<Sequent>,
    /// Each assignment's source constraint and the first per-path fuel value with which it was
    /// derived.
    pub(super) assignments: FxIndexMap<ConstraintAssignment, (ConstraintId, u16)>,
    /// Additional per-path fuel values that can derive an assignment, keyed by its index in
    /// `assignments`. These are stored separately so that branch-local additions can be rolled
    /// back by truncating the set. Only the greatest fuel value participates in further
    /// derivation.
    additional_fuels: Vec<(usize, u16)>,
    /// The amount of global fuel that remains across all assignments and paths.
    remaining_overall_fuel: u16,
    /// Constraints that we have discovered, mapped to whether we have processed them yet. (This
    /// ensures a stable order for all of the derived constraints that we create, while still
    /// letting us create them lazily.)
    discovered: FxIndexMap<ConstraintId, bool>,
    /// Constraint pairs that we have already checked and added to `sequents`.
    elaborated_pairs: FxHashSet<(ConstraintId, ConstraintId)>,

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

impl PathAssignments {
    pub(super) fn new(
        constraints: impl IntoIterator<Item = ConstraintId>,
        independent_typevars: FxHashSet<TypeVarId>,
    ) -> Self {
        let discovered = constraints
            .into_iter()
            .map(|constraint| (constraint, false))
            .collect();
        Self {
            sequents: Vec::default(),
            assignments: FxIndexMap::default(),
            additional_fuels: Vec::default(),
            discovered,
            elaborated_pairs: FxHashSet::default(),
            independent_typevars,
            remaining_overall_fuel: OVERALL_FUEL_BUDGET,
            assignment_queue: VecDeque::default(),
            new_assignments: FxIndexMap::default(),
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
        self.visit_inner(db, env, storage, node, visitor, false)
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
        self.visit_inner(db, env, storage, node, visitor, true)
    }

    fn visit_inner<'db, V>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        visitor: &mut V,
        negated: bool,
    ) -> ControlFlow<V::Break, V::Result>
    where
        V: PathVisitor,
    {
        visitor.visit_node()?;
        match node.node() {
            Node::AlwaysTrue if negated => visitor.visit_unsatisfied(db, storage, self),
            Node::AlwaysTrue => visitor.visit_satisfied(db, storage, self),

            Node::AlwaysFalse if negated => visitor.visit_satisfied(db, storage, self),
            Node::AlwaysFalse => visitor.visit_unsatisfied(db, storage, self),

            Node::Interior(interior) => {
                let interior_value = visitor.enter_interior(db, storage, interior)?;
                let interior = storage.interior_node_data(node);

                let true_subtree = if negated {
                    interior.if_true.or(storage, interior.if_uncertain)
                } else {
                    interior.if_true
                };
                let if_true = self.walk_edge(
                    db,
                    env,
                    storage,
                    interior.constraint.when_true(),
                    |storage, path, new_range, found_conflict| {
                        let subtree = if found_conflict {
                            visitor.visit_impossible(db, storage, path)
                        } else {
                            path.visit_inner(db, env, storage, true_subtree, visitor, negated)
                        };
                        match subtree {
                            ControlFlow::Continue(subtree) => visitor.visit_edge(
                                db,
                                storage,
                                &interior_value,
                                subtree,
                                path,
                                new_range,
                            ),
                            ControlFlow::Break(b) => ControlFlow::Break(b),
                        }
                    },
                )?;

                let if_uncertain = if negated {
                    let subtree = visitor.visit_impossible(db, storage, self)?;
                    visitor.visit_edge(db, storage, &interior_value, subtree, self, 0..0)?
                } else {
                    self.walk_edge(
                        db,
                        env,
                        storage,
                        interior.constraint.when_unconstrained(),
                        |storage, path, new_range, found_conflict| {
                            let subtree = if found_conflict {
                                visitor.visit_impossible(db, storage, path)
                            } else {
                                path.visit_inner(
                                    db,
                                    env,
                                    storage,
                                    interior.if_uncertain,
                                    visitor,
                                    false,
                                )
                            };
                            match subtree {
                                ControlFlow::Continue(subtree) => visitor.visit_edge(
                                    db,
                                    storage,
                                    &interior_value,
                                    subtree,
                                    path,
                                    new_range,
                                ),
                                ControlFlow::Break(b) => ControlFlow::Break(b),
                            }
                        },
                    )?
                };

                let false_subtree = if negated {
                    interior.if_false.or(storage, interior.if_uncertain)
                } else {
                    interior.if_false
                };
                let if_false = self.walk_edge(
                    db,
                    env,
                    storage,
                    interior.constraint.when_false(),
                    |storage, path, new_range, found_conflict| {
                        let subtree = if found_conflict {
                            visitor.visit_impossible(db, storage, path)
                        } else {
                            path.visit_inner(db, env, storage, false_subtree, visitor, negated)
                        };
                        match subtree {
                            ControlFlow::Continue(subtree) => visitor.visit_edge(
                                db,
                                storage,
                                &interior_value,
                                subtree,
                                path,
                                new_range,
                            ),
                            ControlFlow::Break(b) => ControlFlow::Break(b),
                        }
                    },
                )?;

                visitor.leave_interior(
                    db,
                    storage,
                    &interior_value,
                    if_true,
                    if_uncertain,
                    if_false,
                )
            }
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
    /// To handle all of this, you provide a callback. If the path has become impossible, we will
    /// return `None` _without invoking the callback_. If the path does not contain any
    /// contradictions, we will invoke the callback and return its result (wrapped in `Some`).
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
        // Record a snapshot of the assignments that we already knew held — both so that we can
        // pass along the range of which assignments are new, and so that we can reset back to this
        // point before returning.
        let start = self.assignments.len();
        let additional_fuels_start = self.additional_fuels.len();
        let previous_remaining_overall_fuel = self.remaining_overall_fuel;

        // Add the new assignment and anything we can derive from it.
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::PathAssignment",
            before = %format_args!(
                "[{}]",
                self.assignments[..start].iter().map(|(assignment, _)| {
                    assignment.display(db, env, storage)
                }).format(", "),
            ),
            edge = %assignment.display(db, env, storage),
            "walk edge",
        );
        debug_assert!(self.assignment_queue.is_empty());
        self.assignment_queue
            .push_back((assignment, AssignmentFuel::origin()));
        let source_constraint = assignment.constraint();
        let found_conflict = self
            .drain_assignment_queue(db, env, storage, source_constraint)
            .is_err();
        if !found_conflict {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                new = %format_args!(
                    "[{}]",
                    self.assignments[start..].iter().map(|(assignment, _)| {
                        assignment.display(db, env, storage)
                    }).format(", "),
                ),
                "new assignments",
            );
        }
        // Otherwise invoke the callback to keep traversing the BDD. The callback will likely
        // traverse additional edges, which might add more to our `assignments` set. But even
        // if that happens, `start..end` will mark the assignments that were added by the
        // `add_assignment` call above — that is, the new assignment for this edge along with
        // the derived information we inferred from it.
        let end = self.assignments.len();
        let result = f(storage, self, start..end, found_conflict);

        // Reset back to where we were before following this edge, so that the caller can reuse a
        // single instance for the entire BDD traversal.
        self.assignment_queue.clear();
        self.assignments.truncate(start);
        self.additional_fuels.truncate(additional_fuels_start);
        self.remaining_overall_fuel = previous_remaining_overall_fuel;
        result
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
        self.assignments.contains_key(&assignment)
    }

    fn contains_constraint(&self, constraint: ConstraintId) -> bool {
        self.assignment_holds(constraint.when_true())
            || self.assignment_holds(constraint.when_false())
            || self.assignment_holds(constraint.when_unconstrained())
    }

    /// Returns the greatest remaining fuel for any derivation of `assignment` on this path.
    fn max_remaining_fuel_for(&self, assignment: ConstraintAssignment) -> Option<u16> {
        let (index, _, (_, first_fuel)) = self.assignments.get_full(&assignment)?;
        let max_fuel = self
            .additional_fuels
            .iter()
            .filter(|(fuel_index, _)| *fuel_index == index)
            .map(|(_, fuel)| *fuel)
            .fold(*first_fuel, u16::max);
        Some(max_fuel)
    }

    /// Update our sequent map to ensure that it holds all of the sequents that involve the given
    /// constraint. We do not calculate the new sequents directly. Instead, we call
    /// [`SequentMap::for_constraint`] and [`for_constraint_pair`][SequentMap::for_constraint_pair]
    /// to calculate _and cache_ the constraints, so that if we walk another constraint set
    /// containing this constraint, we reuse the work to calculate its sequents.
    fn discover_constraint<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        constraint: ConstraintId,
    ) {
        // If we've already processed this constraint, we can skip it.
        let (constraint_index, existing) = self.discovered.insert_full(constraint, true);
        let already_processed = existing.is_some_and(|existing| existing);
        if already_processed {
            return;
        }

        let single_map = SequentMap::for_constraint(db, env, storage, constraint);
        self.sequents.extend_from_slice(&single_map.sequents);

        for (existing_index, (existing, _)) in self.discovered.iter().enumerate() {
            if *existing == constraint {
                continue;
            }

            let existing_support = storage.constraint_support(*existing);
            let constraint_support = storage.constraint_support(constraint);

            // Independent typevars must be checked for disjoint or invalid constraints, but are
            // otherwise already constrained and do not participate in sequent discovery.
            if !existing_support.overlaps_with(constraint_support)
                && existing_support
                    .iter()
                    .chain(constraint_support.iter())
                    .any(|typevar| self.independent_typevars.contains(&typevar))
                && existing_support.is_complete()
                && constraint_support.is_complete()
            {
                continue;
            }

            if SequentMap::pair_cannot_produce_sequents(db, env, storage, *existing, constraint) {
                continue;
            }

            let (a, b) = if existing_index < constraint_index {
                (*existing, constraint)
            } else {
                (constraint, *existing)
            };
            if !self.elaborated_pairs.insert((a, b)) {
                // We've already elaborated this pair of constraints.
                continue;
            }

            let pair_map = SequentMap::for_constraint_pair(db, env, storage, a, b);
            self.sequents.extend_from_slice(&pair_map.sequents);
        }
    }

    fn drain_assignment_queue<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_constraint: ConstraintId,
    ) -> Result<(), PathAssignmentConflict> {
        while let Some((assignment, fuel)) = self.assignment_queue.pop_front() {
            self.add_assignment(db, env, storage, assignment, source_constraint, fuel)?;
        }
        Ok(())
    }

    /// Adds a new assignment, along with any derived information that we can infer from the new
    /// assignment combined with the assignments we've already seen. If any of this causes the path
    /// to become invalid, due to a contradiction, returns a [`PathAssignmentConflict`] error.
    fn add_assignment<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        assignment: ConstraintAssignment,
        source_constraint: ConstraintId,
        fuel: AssignmentFuel,
    ) -> Result<(), PathAssignmentConflict> {
        if matches!(assignment, ConstraintAssignment::Unconstrained(_)) {
            // An `Unconstrained` assignment means "this constraint can go either way". If there is
            // already any assignment for this constraint (positive, negative, or unconstrained),
            // the existing assignment is at least as informative, and we skip.
            if self.contains_constraint(assignment.constraint()) {
                return Ok(());
            }

            // Since we don't know whether the assignment's constraint holds or not, we cannot
            // derive any additional information from the sequent map. We still want to record the
            // assignment, but as an optimization we can return early without actually querying the
            // sequent map.
            self.assignments
                .insert(assignment, (source_constraint, fuel.remaining));
            return Ok(());
        }

        // First add this assignment. If it causes a conflict, return that as an error.
        if self.assignments.contains_key(&assignment.negated()) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                assignment = %assignment.display(db, env, storage),
                facts = %format_args!(
                    "[{}]",
                    self.assignments.iter().map(|(assignment, _)| {
                        assignment.display(db, env, storage)
                    }).format(", "),
                ),
                "found contradiction",
            );
            return Err(PathAssignmentConflict);
        }

        match self.assignments.entry(assignment) {
            Entry::Vacant(entry) => {
                if let Some(fuel_cost) = fuel.consumed {
                    self.remaining_overall_fuel =
                        match self.remaining_overall_fuel.checked_sub(fuel_cost) {
                            Some(updated_fuel) => updated_fuel,
                            None => return Ok(()),
                        };
                }
                entry.insert((source_constraint, fuel.remaining));
            }

            Entry::Occupied(mut entry) => {
                let index = entry.index();
                let (existing_source_constraint, existing_fuel) = entry.get_mut();

                // If a constraint appears both as an "origin" constraint (it actually appears in
                // the BDD structure) and as a "derived" constraint (we infer it from other
                // constraints), we should prefer the origin source constraint, regardless of which
                // order we encounter the various constraints in the BDD.
                if !fuel.is_derived() {
                    *existing_source_constraint = source_constraint;
                }

                // We've already seen this assignment, and in theory have already queried the
                // sequent map for its consequents, which should let us return early.
                //
                // However, a new derivation chain can replenish the fuel for this assignment,
                // giving it more chances to participate in multi-step sequent chains. That means
                // there might be some consequents that were skipped previously due to a lack of
                // fuel, that can be added now because of the replinished fuel budget.

                // There is another derivation of this assignment that already provides at least as
                // much fuel as this constraint. That means replenishing the fuel won't have any
                // effect.
                if *existing_fuel >= fuel.remaining
                    || self
                        .additional_fuels
                        .iter()
                        .any(|(fuel_index, existing_fuel)| {
                            *fuel_index == index && *existing_fuel >= fuel.remaining
                        })
                {
                    return Ok(());
                }

                // Record the replenished fuel separately so that `walk_edge` can restore the
                // parent branch by truncating `additional_fuels`.
                self.additional_fuels.push((index, fuel.remaining));
            }
        }

        // Then use our sequents to add additional facts that we know to be true.
        //
        // TODO: This is very naive at the moment, partly for expediency, and partly because we
        // don't anticipate the sequent maps to be very large. We might consider avoiding the
        // brute-force search.

        self.new_assignments.clear();
        self.discover_constraint(db, env, storage, assignment.constraint());

        for i in 0..self.sequents.len() {
            let sequent = self.sequents[i];
            self.check_sequent(db, env, storage, sequent)?;
        }

        // If we were able to derive any new assignments from this one, add them to the processing
        // queue.
        self.assignment_queue.extend(self.new_assignments.drain(..));

        Ok(())
    }

    fn enqueue_assignment(&mut self, assignment: ConstraintAssignment, new_fuel: AssignmentFuel) {
        self.new_assignments
            .entry(assignment)
            .and_modify(|existing_fuel| {
                *existing_fuel = std::cmp::max(*existing_fuel, new_fuel);
            })
            .or_insert(new_fuel);
    }

    fn check_sequent<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        sequent: Sequent,
    ) -> Result<(), PathAssignmentConflict> {
        match sequent {
            Sequent::SingleTautology { ante } => {
                self.check_single_tautology(db, env, storage, ante)
            }
            Sequent::PairImpossibility { ante1, ante2 } => {
                self.check_pair_impossibility(db, env, storage, ante1, ante2)
            }
            Sequent::PairImplication { ante1, ante2, post } => {
                self.check_pair_implication(db, env, storage, ante1, ante2, post);
                Ok(())
            }
            Sequent::SingleImplication { ante, post } => {
                self.check_single_implication(db, env, storage, ante, post);
                Ok(())
            }
        }
    }

    fn check_single_tautology<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        ante: ConstraintId,
    ) -> Result<(), PathAssignmentConflict> {
        if self.assignment_holds(ante.when_false()) {
            // The sequent map says (ante1) is always true, and the current path asserts that
            // it's false.
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                ante = %ante.display(db, env, storage),
                facts = %format_args!(
                    "[{}]",
                    self.assignments.iter().map(|(assignment, _)| {
                        assignment.display(db, env, storage)
                    }).format(", "),
                ),
                "found contradiction",
            );
            return Err(PathAssignmentConflict);
        }

        Ok(())
    }

    fn check_pair_impossibility<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        ante1: ConstraintId,
        ante2: ConstraintId,
    ) -> Result<(), PathAssignmentConflict> {
        if self.assignment_holds(ante1.when_true()) && self.assignment_holds(ante2.when_true()) {
            // The sequent map says (ante1 ∧ ante2) is an impossible combination, and the
            // current path asserts that both are true.
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                ante1 = %ante1.display(db, env, storage),
                ante2 = %ante2.display(db, env, storage),
                facts = %format_args!(
                    "[{}]",
                    self.assignments.iter().map(|(assignment, _)| {
                        assignment.display(db, env, storage)
                    }).format(", "),
                ),
                "found contradiction",
            );
            return Err(PathAssignmentConflict);
        }

        Ok(())
    }

    fn check_pair_implication<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        ante1: ConstraintId,
        ante2: ConstraintId,
        post: ConstraintId,
    ) {
        let Some(ante1_fuel) = self.max_remaining_fuel_for(ante1.when_true()) else {
            return;
        };
        let Some(ante2_fuel) = self.max_remaining_fuel_for(ante2.when_true()) else {
            return;
        };
        let available_fuel = ante1_fuel.min(ante2_fuel);
        let (ante1_constructor_depth, _) = storage.cached_constraint_bound_depth(db, env, ante1);
        let (ante2_constructor_depth, _) = storage.cached_constraint_bound_depth(db, env, ante2);
        let antecedent_constructor_depth = ante1_constructor_depth.max(ante2_constructor_depth);
        let fuel_cost = storage.sequent_fuel_cost(db, env, post, antecedent_constructor_depth);
        if let Some(post_fuel) = available_fuel.checked_sub(fuel_cost) {
            self.enqueue_assignment(
                post.when_true(),
                AssignmentFuel::derived(fuel_cost, post_fuel),
            );
        }
    }

    fn check_single_implication<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        ante: ConstraintId,
        post: ConstraintId,
    ) {
        let Some(available_fuel) = self.max_remaining_fuel_for(ante.when_true()) else {
            return;
        };
        let ante_data = storage.constraint_data(ante);
        let (antecedent_constructor_depth, _) =
            storage.cached_constraint_bound_depth(db, env, ante);
        let post_data = storage.constraint_data(post);
        let fuel_cost = if post_data.is_bound_projection_of(db, ante_data) {
            1
        } else {
            storage.sequent_fuel_cost(db, env, post, antecedent_constructor_depth)
        };
        if let Some(post_fuel) = available_fuel.checked_sub(fuel_cost) {
            self.enqueue_assignment(
                post.when_true(),
                AssignmentFuel::derived(fuel_cost, post_fuel),
            );
        }
    }
}

#[derive(Debug)]
struct PathAssignmentConflict;

#[cfg(test)]
mod tests {
    use super::super::solutions::SolutionWalker;
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
        ConstraintSet::constrain_typevar(db, &env, builder, bound_typevar, ty, ty)
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
        let expected = PathBounds::compute(
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
}
