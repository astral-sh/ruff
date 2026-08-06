use rustc_hash::FxHashSet;

use crate::types::constraints::support::Support;
use crate::types::constraints::{
    ALWAYS_FALSE, ConstraintAssignment, ConstraintBoundsBuilder, ConstraintId,
    ConstraintSetStorage, Node, NodeId, PathAssignments, PathBounds,
};
use crate::types::typevar::TypeVarSet;
use crate::types::visitor::any_over_type;
use crate::types::{BoundTypeVarInstance, DynamicType, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

pub(super) struct SolutionWalker<'db> {
    inferable: TypeVarSet<'db>,
    inferable_support: Support,
    source_orders: FxIndexSet<ConstraintId>,
    explored_nodes: FxHashSet<(NodeId, Vec<(ConstraintAssignment, ConstraintId)>)>,
    sorted_paths: Vec<Vec<(ConstraintId, usize)>>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source_orders: FxIndexSet<ConstraintId>,
    ) -> Self {
        let inferable_support = Support::from_typevar_set(db, storage, inferable);
        Self {
            inferable,
            inferable_support,
            source_orders,
            explored_nodes: FxHashSet::default(),
            sorted_paths: Vec::default(),
        }
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
    fn constrained_assignments_mentioning(
        storage: &ConstraintSetStorage<'db>,
        path: &PathAssignments,
        support: &Support,
    ) -> impl Iterator<Item = (ConstraintAssignment, ConstraintId)> {
        path.assignments
            .iter()
            .filter_map(|(assignment, (source_constraint, _))| {
                let constraint = assignment.as_constrained()?;
                let constraint_support = storage.constraint_support(constraint);
                constraint_support
                    .overlaps_with(support)
                    .then_some((*assignment, *source_constraint))
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
        let mut relevant_path: Vec<_> =
            Self::constrained_assignments_mentioning(storage, path, &relevant_typevars).collect();
        relevant_path.sort_unstable_by_key(|(assignment, _)| assignment.constraint().ordering());
        let key = (node, relevant_path);
        if !self.explored_nodes.insert(key) {
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
        if !visible_typevars.overlaps_with(node_support) {
            // This node cannot affect the solution we've found. Make sure that the node has _at
            // least one_ satisfiable path, without walking them all. As long as it does, we can
            // report the solution we have so far as-is.
            if Self::node_is_satisfiable_on_path(db, env, storage, path, node) {
                self.found_satisfied_path(storage, path, &visible_typevars);
            }
            return;
        }

        // At this point we actually have to walk the outgoing edges of this node.
        let interior = storage.interior_node_data(node);
        path.walk_edge(
            db,
            env,
            storage,
            interior.constraint.when_true(),
            |storage, path, _new_range, found_conflict| {
                if !found_conflict {
                    self.visit_node(db, env, storage, path, interior.if_true);
                }
            },
        );
        path.walk_edge(
            db,
            env,
            storage,
            interior.constraint.when_unconstrained(),
            |storage, path, _new_range, found_conflict| {
                if !found_conflict {
                    self.visit_node(db, env, storage, path, interior.if_uncertain);
                }
            },
        );
        path.walk_edge(
            db,
            env,
            storage,
            interior.constraint.when_false(),
            |storage, path, _new_range, found_conflict| {
                if !found_conflict {
                    self.visit_node(db, env, storage, path, interior.if_false);
                }
            },
        );
    }

    /// Returns if there is _any_ satisfiable path in `node`, assuming that the assignments in
    /// `path` already hold. Avoids walking the entire subtree if possible, by returning early once
    /// we find the first satisfied path.
    fn node_is_satisfiable_on_path(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &mut PathAssignments,
        node: NodeId,
    ) -> bool {
        match node.node() {
            Node::AlwaysTrue => return true,
            Node::AlwaysFalse => return false,
            Node::Interior(_) => {}
        }

        let interior = storage.interior_node_data(node);

        let true_is_satisfied = path.walk_edge(
            db,
            env,
            storage,
            interior.constraint.when_true(),
            |storage, path, _new_range, found_conflict| {
                if found_conflict {
                    false
                } else {
                    Self::node_is_satisfiable_on_path(db, env, storage, path, interior.if_true)
                }
            },
        );
        if true_is_satisfied {
            return true;
        }

        let uncertain_is_satisfied = path.walk_edge(
            db,
            env,
            storage,
            interior.constraint.when_unconstrained(),
            |storage, path, _new_range, found_conflict| {
                if found_conflict {
                    false
                } else {
                    Self::node_is_satisfiable_on_path(db, env, storage, path, interior.if_uncertain)
                }
            },
        );
        if uncertain_is_satisfied {
            return true;
        }

        path.walk_edge(
            db,
            env,
            storage,
            interior.constraint.when_false(),
            |storage, path, _new_range, found_conflict| {
                if found_conflict {
                    false
                } else {
                    Self::node_is_satisfiable_on_path(db, env, storage, path, interior.if_false)
                }
            },
        )
    }

    fn found_satisfied_path(
        &mut self,
        storage: &ConstraintSetStorage<'db>,
        path: &PathAssignments,
        visible_typevars: &Support,
    ) {
        let mut path: Vec<_> =
            Self::constrained_assignments_mentioning(storage, path, visible_typevars)
                .filter(|(assignment, _)| assignment.is_positive())
                .map(|(assignment, source_constraint)| {
                    let source_order = self
                        .source_orders
                        .get_index_of(&source_constraint)
                        .expect("every TDD constraint should have a source order");
                    (assignment.constraint(), source_order)
                })
                .collect();
        path.sort_by_key(|(_, source_order)| *source_order);
        self.sorted_paths.push(path);
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
            let source_orders1 = path1.iter().map(|(_, source_order)| *source_order);
            let source_orders2 = path2.iter().map(|(_, source_order)| *source_order);
            source_orders1.cmp(source_orders2)
        });
        self.sorted_paths.dedup_by(|path1, path2| {
            let source_orders1 = path1.iter();
            let source_orders2 = path2.iter();
            source_orders1.eq(source_orders2)
        });

        let mut result = Vec::with_capacity(self.sorted_paths.len());
        let mut any_constrained_solutions = false;
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();
        let is_bare_inferable_typevar = |ty: Type<'db>| {
            ty.as_typevar()
                .is_some_and(|typevar| typevar.is_inferable(db, self.inferable))
        };
        let contains_unspecialized_typevar = |ty: Type<'db>| {
            any_over_type(db, env, ty, false, |nested| {
                matches!(nested, Type::Dynamic(DynamicType::UnspecializedTypeVar))
            })
        };

        for path in self.sorted_paths {
            mappings.clear();
            for (constraint, _) in path {
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

                // An unspecialized outer type variable carries neither an identity nor concrete
                // inference evidence. A provisional lambda parameter is different: its enclosing
                // callable still provides a useful concrete bound and must be preserved.
                if let Some(lower) = constraint.bounds.lower
                    && !contains_unspecialized_typevar(lower)
                {
                    let bounds = mappings.entry(typevar).or_default();
                    bounds.add_lower(db, env, lower);

                    if let Type::TypeVar(lower_bound_typevar) = lower {
                        let bounds = mappings.entry(lower_bound_typevar).or_default();
                        bounds.add_upper(db, env, Type::TypeVar(typevar));
                    }
                }

                if let Some(upper) = constraint.bounds.upper
                    && !contains_unspecialized_typevar(upper)
                {
                    let bounds = mappings.entry(typevar).or_default();
                    bounds.add_upper(db, env, upper);

                    if let Type::TypeVar(upper_bound_typevar) = upper {
                        let bounds = mappings.entry(upper_bound_typevar).or_default();
                        bounds.add_lower(db, env, Type::TypeVar(typevar));
                    }
                }
            }

            if !mappings.is_empty() {
                any_constrained_solutions = true;
            }

            let path_bounds = mappings
                .drain(..)
                .map(|(bound_typevar, bounds)| bounds.finish(db, env, bound_typevar))
                .collect();
            result.push(path_bounds);
        }

        if !any_constrained_solutions {
            return PathBounds::Unconstrained;
        }

        PathBounds::Constrained(result.into_boxed_slice())
    }
}
