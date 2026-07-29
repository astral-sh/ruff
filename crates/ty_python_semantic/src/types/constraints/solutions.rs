use std::ops::ControlFlow;

use rustc_hash::FxHashSet;

use crate::types::constraints::support::Support;
use crate::types::constraints::{
    ALWAYS_FALSE, ConstraintAssignment, ConstraintBoundsBuilder, ConstraintId,
    ConstraintSetStorage, Node, NodeId, PathAssignments, PathBounds, SolutionLimits,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

struct SolutionWalker<'db> {
    inferable: TypeVarSet<'db>,
    inferable_support: Support,
    source_orders: FxIndexSet<ConstraintId>,
    explored_nodes: FxHashSet<(NodeId, Vec<(ConstraintAssignment, ConstraintId)>)>,
    sorted_paths: Vec<Vec<(ConstraintId, usize)>>,
}

impl<'db> SolutionWalker<'db> {
    /// Returns an iterator of the positive and negative constraints on the current path
    fn constrained_assignments(
        &self,
        path: &PathAssignments,
    ) -> impl Iterator<Item = ConstraintId> + Clone {
        path.assignments
            .iter()
            .filter_map(|(assignment, _)| assignment.as_constrained())
    }

    /// Returns an iterator of the constraints on the current path that mention any typevar in the
    /// given support
    fn constrained_assignments_mentioning(
        &self,
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

    fn visit_node<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &mut PathAssignments,
        node: NodeId,
        limits: &mut L,
    ) -> ControlFlow<L::Break> {
        limits.visit_node()?;
        if node == ALWAYS_FALSE {
            return ControlFlow::Continue(());
        }

        // First see if we've already visited this node on an "equivalent" path, where we only
        // consider the typevars that can affect the solutions we'd find if we were to continue
        // walking down the node.
        let mut relevant_typevars = self.inferable_support.clone();
        let node_support = storage.node_support(node);
        if let Some(node_support) = node_support {
            relevant_typevars |= node_support;
        }
        relevant_typevars.close_over_constraints(storage, self.constrained_assignments(path));
        let mut relevant_path: Vec<_> = self
            .constrained_assignments_mentioning(storage, path, &relevant_typevars)
            .collect();
        relevant_path.sort_unstable_by_key(|(assignment, _)| assignment.constraint().ordering());
        let key = (node, relevant_path);
        if !self.explored_nodes.insert(key) {
            return ControlFlow::Continue(());
        }

        // If the current node is ALWAYS_TRUE, we can immediately report the current solution.
        // (We'll only have a Some(node_support) is the node is non-terminal, and we ruled out
        // ALWAYS_FALSE up above.)
        let Some(node_support) = node_support else {
            limits.satisfied_path()?;
            self.found_satisfied_path(storage, path, relevant_typevars);
            return ControlFlow::Continue(());
        };

        // Next see if anything in this node can affect the solution we've already calculated on
        // the current path.
        let mut visible_typevars = self.inferable_support.clone();
        visible_typevars.close_over_constraints(storage, self.constrained_assignments(path));
        if !visible_typevars.overlaps_with(node_support) {
            // This node cannot affect the solution we've found. Make sure that the node has _at
            // least one_ satisfiable path, without walking them all. As long as it does, we can
            // report the solution we have so far as-is.
            if self.node_is_satisfiable_on_path(db, env, storage, path, node) {
                limits.satisfied_path()?;
                self.found_satisfied_path(storage, path, visible_typevars);
            }
            return ControlFlow::Continue(());
        }

        // At this point we actually have to walk the outgoing edges of this node.
        let interior = storage.interior_node_data(node);
        let constraint = interior.constraint;
        for (assignment, child) in [
            (constraint.when_true(), interior.if_true),
            (constraint.when_unconstrained(), interior.if_uncertain),
            (constraint.when_false(), interior.if_false),
        ] {
            path.walk_edge(
                db,
                env,
                storage,
                assignment,
                |storage, path, _new_range, found_conflict| {
                    if !found_conflict {
                        self.visit_node(db, env, storage, path, child, limits)?;
                    }
                    ControlFlow::Continue(())
                },
            )?;
        }
        ControlFlow::Continue(())
    }

    /// Returns if there is _any_ satisfiable path in `node`, assuming that the assignments in
    /// `path` already hold. Avoids walking the entire subtree if possible, by returning early once
    /// we find the first satisfied path.
    fn node_is_satisfiable_on_path(
        &mut self,
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
                    self.node_is_satisfiable_on_path(db, env, storage, path, interior.if_true)
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
                    self.node_is_satisfiable_on_path(db, env, storage, path, interior.if_uncertain)
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
                    self.node_is_satisfiable_on_path(db, env, storage, path, interior.if_false)
                }
            },
        )
    }

    fn found_satisfied_path(
        &mut self,
        storage: &ConstraintSetStorage<'db>,
        path: &PathAssignments,
        visible_typevars: Support,
    ) {
        let mut path: Vec<_> = self
            .constrained_assignments_mentioning(storage, path, &visible_typevars)
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
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();

        for path in self.sorted_paths {
            mappings.clear();
            for (constraint, _) in path {
                let constraint = storage.constraint_data(constraint);
                let typevar = constraint.typevar;
                if let Some(lower) = constraint.bounds.lower {
                    if typevar.is_inferable(db, self.inferable) {
                        let bounds = mappings.entry(typevar).or_default();
                        bounds.add_lower(db, env, lower);
                    }

                    if let Type::TypeVar(lower_bound_typevar) = lower.ty()
                        && lower_bound_typevar.is_inferable(db, self.inferable)
                    {
                        let bounds = mappings.entry(lower_bound_typevar).or_default();
                        bounds.add_upper(db, env, lower.with_type(Type::TypeVar(typevar)));
                    }
                }

                if let Some(upper) = constraint.bounds.upper {
                    if typevar.is_inferable(db, self.inferable) {
                        let bounds = mappings.entry(typevar).or_default();
                        bounds.add_upper(db, env, upper);
                    }

                    if let Type::TypeVar(upper_bound_typevar) = upper.ty()
                        && upper_bound_typevar.is_inferable(db, self.inferable)
                    {
                        let bounds = mappings.entry(upper_bound_typevar).or_default();
                        bounds.add_lower(db, env, upper.with_type(Type::TypeVar(typevar)));
                    }
                }
            }

            // If any solution path is empty, the overall solution is "unconstrained".
            if mappings.is_empty() {
                return PathBounds::Unconstrained;
            }

            let path_bounds = mappings
                .drain(..)
                .map(|(bound_typevar, bounds)| bounds.finish(db, env, bound_typevar))
                .collect();
            result.push(path_bounds);
        }

        PathBounds::Constrained(result.into_boxed_slice())
    }
}
