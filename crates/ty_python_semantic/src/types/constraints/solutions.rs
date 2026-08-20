use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::types::constraints::support::Support;
use crate::types::constraints::{
    ALWAYS_FALSE, ConstraintAssignment, ConstraintBoundsBuilder, ConstraintId,
    ConstraintProvenance, ConstraintSet, ConstraintSetStorage, GradualVariableId, Node, NodeId,
    PathAssignments, PathBounds,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

/// A satisfying constraint path.
struct ConstraintPath {
    constraints: Vec<(ConstraintId, usize, ConstraintProvenance)>,
    /// The gradual occurrences whose materializations satisfy this constraint path.
    materialization_origins: GradualOrigins,
}

type PathAssignmentKey = (ConstraintAssignment, ConstraintId, ConstraintProvenance);
type GradualOrigins = SmallVec<[GradualVariableId; 2]>;

pub(super) struct SolutionWalker<'db> {
    inferable: TypeVarSet<'db>,
    inferable_support: Support,
    source_orders: FxIndexSet<ConstraintId>,
    relevant_gradual_origins: FxHashMap<GradualVariableId, GradualVariableId>,
    canonical_gradual_constraints: FxHashMap<ConstraintId, ConstraintId>,
    relevant_gradual_nodes: FxHashMap<NodeId, bool>,
    explored_nodes: FxHashSet<(NodeId, Vec<PathAssignmentKey>)>,
    explored_gradual_nodes: FxHashSet<(NodeId, Vec<PathAssignmentKey>, GradualOrigins)>,
    sorted_paths: Vec<ConstraintPath>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source_orders: FxIndexSet<ConstraintId>,
    ) -> Self {
        let inferable_support = Support::from_typevar_set(db, storage, inferable);
        let mut relevant_gradual_origins = FxHashMap::default();
        let mut canonical_gradual_constraints = FxHashMap::default();
        if storage.has_gradual_variables() {
            let mut relevant_support = inferable_support.clone();
            relevant_support.close_over_constraints(storage, &source_orders.iter().copied());
            let has_incomplete_support = source_orders
                .iter()
                .any(|&constraint| !storage.constraint_support(constraint).is_complete());
            let mut unrelated_constraints = FxHashMap::default();
            for &constraint_id in &source_orders {
                let Some(constraint) = storage.constraint_data(constraint_id).as_typevar() else {
                    continue;
                };

                if has_incomplete_support
                    || storage
                        .constraint_support(constraint_id)
                        .overlaps_with(&relevant_support)
                {
                    for origin in [constraint.provenance.lower, constraint.provenance.upper]
                        .into_iter()
                        .flatten()
                    {
                        relevant_gradual_origins.insert(origin, origin);
                    }
                } else {
                    let representative = *unrelated_constraints
                        .entry((constraint.typevar, constraint.bounds))
                        .or_insert(constraint_id);
                    if representative != constraint_id {
                        canonical_gradual_constraints.insert(constraint_id, representative);
                    }
                }
            }
        }

        let mut walker = Self {
            inferable,
            inferable_support,
            source_orders,
            relevant_gradual_origins,
            canonical_gradual_constraints,
            relevant_gradual_nodes: FxHashMap::default(),
            explored_nodes: FxHashSet::default(),
            explored_gradual_nodes: FxHashSet::default(),
            sorted_paths: Vec::default(),
        };
        if walker.relevant_gradual_origins.len() < 2 {
            return walker;
        }

        let mut origin_uses = FxHashMap::default();
        let mut has_incomplete_support = false;
        for &constraint_id in &walker.source_orders {
            let Some(constraint) = storage.constraint_data(constraint_id).as_typevar() else {
                continue;
            };

            let support = storage.constraint_support(constraint_id);
            has_incomplete_support |= !support.is_complete();

            for origin in [constraint.provenance.lower, constraint.provenance.upper]
                .into_iter()
                .flatten()
            {
                *origin_uses.entry(origin).or_insert(0) += 1;
            }
        }
        if has_incomplete_support {
            return walker;
        }

        // Consecutive occurrences that supply the same bound are interchangeable.
        // Crossing another bound would change source order.
        let mut representative = None;
        for &constraint_id in &walker.source_orders {
            let Some(constraint) = storage.constraint_data(constraint_id).as_typevar() else {
                continue;
            };
            let Some(origin) = constraint.provenance.lower.xor(constraint.provenance.upper) else {
                representative = None;
                continue;
            };
            if !walker.relevant_gradual_origins.contains_key(&origin) {
                representative = None;
                continue;
            }

            let support = storage.constraint_support(constraint_id);
            if origin_uses.get(&origin).copied() != Some(1) || support.iter().nth(1).is_some() {
                representative = None;
                continue;
            }

            let key = (
                constraint.typevar,
                constraint.bounds,
                constraint.provenance.lower.is_some(),
                storage.gradual_origin(origin).ty,
            );
            if let Some((previous, representative_constraint, representative_origin)) =
                representative
                && previous == key
            {
                walker
                    .canonical_gradual_constraints
                    .insert(constraint_id, representative_constraint);
                walker
                    .relevant_gradual_origins
                    .insert(origin, representative_origin);
            } else {
                representative = Some((key, constraint_id, origin));
            }
        }

        walker
    }

    pub(super) fn canonicalize_node(
        &self,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
    ) -> NodeId {
        if self.canonical_gradual_constraints.is_empty() {
            return node;
        }

        let constraints = self
            .canonical_gradual_constraints
            .iter()
            .map(|(&constraint, &representative)| {
                (constraint, Node::new_constraint(storage, representative))
            })
            .collect();
        ConstraintSet::remap_nodes(storage, node, &constraints, &mut FxHashMap::default())
    }

    /// Returns an iterator of the positive and negative constraints on the current path
    fn constrained_assignments(
        path: &PathAssignments,
    ) -> impl Iterator<Item = ConstraintId> + Clone {
        path.assignments
            .iter()
            .filter_map(|(assignment, _)| assignment.as_constrained())
    }

    /// Returns an iterator of the constraints on the current path that mention any typevar in the
    /// given support
    fn constrained_assignments_mentioning<'a>(
        &'a self,
        storage: &'a ConstraintSetStorage<'db>,
        path: &'a PathAssignments,
        support: &'a Support,
    ) -> impl Iterator<Item = PathAssignmentKey> + 'a {
        path.assignments
            .iter()
            .filter_map(move |(assignment, (source_constraint, _))| {
                let constraint = assignment.as_constrained()?;
                if !storage
                    .constraint_support(constraint)
                    .overlaps_with(support)
                {
                    return None;
                }

                let provenance = path.constraint_provenance(*assignment);
                if self.canonical_gradual_constraints.is_empty() {
                    return Some((*assignment, *source_constraint, provenance));
                }

                let constraint = self.canonical_constraint(constraint);
                let assignment = if assignment.is_positive() {
                    ConstraintAssignment::Positive(constraint)
                } else {
                    ConstraintAssignment::Negative(constraint)
                };

                Some((
                    assignment,
                    self.canonical_constraint(*source_constraint),
                    provenance.map(|origin| self.canonical_origin(origin)),
                ))
            })
    }

    pub(super) fn visit_node(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &mut PathAssignments,
        node: NodeId,
    ) {
        if node == ALWAYS_FALSE {
            return;
        }

        // First see if we've already visited this node on an "equivalent" path, where we only
        // consider the typevars that can affect the solutions we'd find if we were to continue
        // walking down the node.
        let mut relevant_typevars = self.inferable_support.clone();
        let node_support = storage.node_support(node);
        if let Some(node_support) = node_support {
            relevant_typevars |= node_support;
        }
        relevant_typevars.close_over_constraints(storage, &Self::constrained_assignments(path));
        let mut relevant_path: Vec<_> = self
            .constrained_assignments_mentioning(storage, path, &relevant_typevars)
            .collect();
        relevant_path.sort_unstable_by_key(|(assignment, _, _)| assignment.constraint().ordering());
        if !self.canonical_gradual_constraints.is_empty() {
            relevant_path.dedup();
        }
        let is_unexplored = if self.relevant_gradual_origins.is_empty() {
            self.explored_nodes.insert((node, relevant_path))
        } else {
            let mut gradual_origins = GradualOrigins::new();
            for origin in path.gradual_origins(storage) {
                if let Some(&canonical) = self.relevant_gradual_origins.get(&origin)
                    && !gradual_origins.contains(&canonical)
                {
                    gradual_origins.push(canonical);
                }
            }
            gradual_origins.sort_unstable();
            self.explored_gradual_nodes
                .insert((node, relevant_path, gradual_origins))
        };
        if !is_unexplored {
            return;
        }

        // If the current node is ALWAYS_TRUE, we can immediately report the current solution.
        // (We'll only have a Some(node_support) is the node is non-terminal, and we ruled out
        // ALWAYS_FALSE up above.)
        let Some(node_support) = node_support else {
            self.found_satisfied_path(storage, path, &relevant_typevars);
            return;
        };

        // Next see if anything in this node can affect the solution we've already calculated on
        // the current path.
        let mut visible_typevars = self.inferable_support.clone();
        visible_typevars.close_over_constraints(storage, &Self::constrained_assignments(path));
        if !visible_typevars.overlaps_with(node_support)
            && !self.node_has_relevant_gradual_origin(storage, node)
        {
            // This node cannot affect the solution we've found. Make sure that the node has _at
            // least one_ satisfiable path, without walking them all. As long as it does, we can
            // report the solution we have so far as-is.
            self.visit_first_satisfied_path(db, env, storage, path, node, &visible_typevars);
            return;
        }

        // At this point we actually have to walk the outgoing edges of this node.
        let interior = storage.interior_node_data(node);
        let edges = [
            (interior.constraint.when_true(), interior.if_true),
            (
                interior.constraint.when_unconstrained(),
                interior.if_uncertain,
            ),
            (interior.constraint.when_false(), interior.if_false),
        ];
        for (assignment, child) in edges {
            path.walk_edge(
                db,
                env,
                storage,
                assignment,
                |storage, path, _new_range, found_conflict| {
                    if !found_conflict {
                        self.visit_node(db, env, storage, path, child);
                    }
                },
            );
        }
    }

    /// Whether this subtree can distinguish gradual occurrences used by an inferred bound.
    fn node_has_relevant_gradual_origin(
        &mut self,
        storage: &ConstraintSetStorage<'db>,
        node: NodeId,
    ) -> bool {
        if self.relevant_gradual_origins.is_empty() || node.is_terminal() {
            return false;
        }
        if let Some(has_relevant_origin) = self.relevant_gradual_nodes.get(&node) {
            return *has_relevant_origin;
        }

        let interior = storage.interior_node_data(node);
        let has_relevant_origin = storage
            .constraint_data(interior.constraint)
            .as_gradual()
            .is_some_and(|variable| self.relevant_gradual_origins.contains_key(&variable.origin))
            || self.node_has_relevant_gradual_origin(storage, interior.if_true)
            || self.node_has_relevant_gradual_origin(storage, interior.if_uncertain)
            || self.node_has_relevant_gradual_origin(storage, interior.if_false);
        self.relevant_gradual_nodes
            .insert(node, has_relevant_origin);
        has_relevant_origin
    }

    /// Records the first satisfying path without exploring the remaining subtree.
    fn visit_first_satisfied_path(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &mut PathAssignments,
        node: NodeId,
        visible_typevars: &Support,
    ) -> bool {
        match node.node() {
            Node::AlwaysTrue => {
                self.found_satisfied_path(storage, path, visible_typevars);
                return true;
            }
            Node::AlwaysFalse => return false,
            Node::Interior(_) => {}
        }

        let interior = storage.interior_node_data(node);
        for (assignment, child) in [
            (interior.constraint.when_true(), interior.if_true),
            (
                interior.constraint.when_unconstrained(),
                interior.if_uncertain,
            ),
            (interior.constraint.when_false(), interior.if_false),
        ] {
            let satisfied = path.walk_edge(
                db,
                env,
                storage,
                assignment,
                |storage, path, _new_range, found_conflict| {
                    !found_conflict
                        && self.visit_first_satisfied_path(
                            db,
                            env,
                            storage,
                            path,
                            child,
                            visible_typevars,
                        )
                },
            );

            if satisfied {
                return true;
            }
        }

        false
    }

    fn found_satisfied_path(
        &mut self,
        storage: &ConstraintSetStorage<'db>,
        path: &PathAssignments,
        visible_typevars: &Support,
    ) {
        let mut constraints: Vec<_> = self
            .constrained_assignments_mentioning(storage, path, visible_typevars)
            .filter(|(assignment, _, _)| assignment.is_positive())
            .map(|(assignment, source_constraint, provenance)| {
                let source_order = self
                    .source_orders
                    .get_index_of(&source_constraint)
                    .expect("every TDD constraint should have a source order");
                (assignment.constraint(), source_order, provenance)
            })
            .collect();
        // Preserve the order of tied constraints when constructing unions and intersections.
        constraints.sort_by_key(|(_, source_order, _)| *source_order);
        if !self.canonical_gradual_constraints.is_empty() {
            constraints.dedup();
        }

        let mut materialization_origins = SmallVec::new();
        if storage.has_gradual_variables() {
            for origin in path.gradual_origins(storage) {
                let origin = self.canonical_origin(origin);
                if !materialization_origins.contains(&origin) {
                    materialization_origins.push(origin);
                }
            }
        }

        self.sorted_paths.push(ConstraintPath {
            constraints,
            materialization_origins,
        });
    }

    fn canonical_origin(&self, origin: GradualVariableId) -> GradualVariableId {
        self.relevant_gradual_origins
            .get(&origin)
            .copied()
            .unwrap_or(origin)
    }

    fn canonical_constraint(&self, constraint: ConstraintId) -> ConstraintId {
        self.canonical_gradual_constraints
            .get(&constraint)
            .copied()
            .unwrap_or(constraint)
    }

    pub(super) fn finish(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
    ) -> PathBounds<'db> {
        if self.sorted_paths.is_empty() {
            return PathBounds::Unsatisfiable;
        }

        self.sorted_paths.sort_by(|path1, path2| {
            let source_orders1 = path1
                .constraints
                .iter()
                .map(|(_, source_order, _)| *source_order);
            let source_orders2 = path2
                .constraints
                .iter()
                .map(|(_, source_order, _)| *source_order);
            source_orders1.cmp(source_orders2).then(
                path1
                    .materialization_origins
                    .is_empty()
                    .cmp(&path2.materialization_origins.is_empty())
                    .reverse(),
            )
        });
        self.sorted_paths.dedup_by(|path, retained| {
            if path.constraints != retained.constraints {
                return false;
            }

            // A materialization is required only if every equivalent path depends on it.
            retained
                .materialization_origins
                .retain(|origin| path.materialization_origins.contains(origin));
            true
        });

        let mut result = Vec::with_capacity(self.sorted_paths.len());
        let mut any_constrained_solutions = false;
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();
        let is_bare_inferable_typevar = |ty: Type<'db>| {
            ty.as_typevar()
                .is_some_and(|typevar| typevar.is_inferable(db, self.inferable))
        };

        for path in self.sorted_paths {
            mappings.clear();
            for (constraint, _, provenance) in path.constraints {
                let Some(constraint) = storage.constraint_data(constraint).as_typevar() else {
                    continue;
                };
                let typevar = constraint.typevar;

                // A direct relationship between an inferable and non-inferable typevar must
                // contribute bounds for both endpoints. Contextual inference relies on the
                // reverse, non-inferable binding to preserve relationships to outer typevars.
                // Constraints on unrelated non-inferable typevars must not contribute bindings.
                if !typevar.is_inferable(db, self.inferable)
                    && !constraint
                        .bounds
                        .lower
                        .is_some_and(is_bare_inferable_typevar)
                    && !constraint
                        .bounds
                        .upper
                        .is_some_and(is_bare_inferable_typevar)
                {
                    continue;
                }

                if let Some(lower) = constraint.bounds.lower {
                    let bounds = mappings.entry(typevar).or_default();
                    let origin = provenance
                        .lower
                        .map(|origin| storage.gradual_origin(origin));
                    bounds.add_lower(lower, origin);

                    if let Type::TypeVar(lower_bound_typevar) = lower {
                        let bounds = mappings.entry(lower_bound_typevar).or_default();
                        bounds.add_upper(Type::TypeVar(typevar), origin);
                    }
                }

                if let Some(upper) = constraint.bounds.upper {
                    let bounds = mappings.entry(typevar).or_default();
                    let origin = provenance
                        .upper
                        .map(|origin| storage.gradual_origin(origin));
                    bounds.add_upper(upper, origin);

                    if let Type::TypeVar(upper_bound_typevar) = upper {
                        let bounds = mappings.entry(upper_bound_typevar).or_default();
                        bounds.add_lower(Type::TypeVar(typevar), origin);
                    }
                }
            }

            any_constrained_solutions |= !mappings.is_empty();

            let path_bounds: Box<[_]> = mappings
                .drain(..)
                .map(|(bound_typevar, bounds)| {
                    let mut bound = bounds.finish(db, env, bound_typevar);
                    bound.provenance.set_materialization_origins(
                        path.materialization_origins
                            .iter()
                            .map(|&origin| storage.gradual_origin(origin)),
                    );
                    bound
                })
                .collect();
            result.push(path_bounds);
        }

        if !any_constrained_solutions {
            return PathBounds::Unconstrained;
        }

        if storage.has_gradual_variables() {
            result.dedup_by(|left, right| !left.is_empty() && left == right);
        }

        PathBounds::Constrained(result.into_boxed_slice())
    }
}
