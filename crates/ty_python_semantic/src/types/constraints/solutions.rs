use std::marker::PhantomData;
use std::ops::ControlFlow;

use crate::types::constraints::support::Support;
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, Constraint, ConstraintAssignment, ConstraintBound,
    ConstraintBoundsBuilder, ConstraintId, ConstraintSetStorage, NodeId, PathAssignments,
    PathBounds, SolutionLimits,
};
use crate::types::typevar::TypeVarBoundOrConstraints;
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,
    sorted_paths: Vec<Vec<(ConstraintId, usize)>>,
    _phantom: PhantomData<&'db ()>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>) -> Self {
        Self {
            source_orders,
            sorted_paths: Vec::default(),
            _phantom: PhantomData,
        }
    }

    #[expect(clippy::too_many_arguments)]
    pub(super) fn visit_node<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        all_typevars: Option<&Support>,
        node: NodeId,
    ) -> ControlFlow<L::Break> {
        self.visit_node_and_then(
            db,
            env,
            storage,
            limits,
            path,
            node,
            &mut |this, storage, limits, path| match all_typevars {
                Some(all_typevars) => {
                    this.validate_satisfied_path(db, env, storage, limits, path, all_typevars)
                }
                None => this.found_satisfied_path(limits, path),
            },
        )
    }

    #[expect(clippy::too_many_arguments)]
    #[expect(clippy::type_complexity)]
    fn visit_node_and_then<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        node: NodeId,
        process_satisfied: &mut dyn FnMut(
            &mut Self,
            &mut ConstraintSetStorage<'db>,
            &mut L,
            &mut PathAssignments,
        ) -> ControlFlow<L::Break>,
    ) -> ControlFlow<L::Break> {
        limits.visit_node()?;
        if node == ALWAYS_FALSE {
            return ControlFlow::Continue(());
        }

        // If the current node is ALWAYS_TRUE, we can immediately report the current solution.
        if node == ALWAYS_TRUE {
            return process_satisfied(self, storage, limits, path);
        }

        // At this point we actually have to walk the outgoing edges of this node.
        let interior = storage.interior_node_data(node);
        self.visit_edge(
            db,
            env,
            storage,
            limits,
            path,
            interior.constraint.when_true(),
            interior.if_true,
            process_satisfied,
        )?;
        self.visit_edge(
            db,
            env,
            storage,
            limits,
            path,
            interior.constraint.when_unconstrained(),
            interior.if_uncertain,
            process_satisfied,
        )?;
        self.visit_edge(
            db,
            env,
            storage,
            limits,
            path,
            interior.constraint.when_false(),
            interior.if_false,
            process_satisfied,
        )?;
        ControlFlow::Continue(())
    }

    #[expect(clippy::too_many_arguments)]
    #[expect(clippy::type_complexity)]
    fn visit_edge<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        assignment: ConstraintAssignment,
        child: NodeId,
        process_satisfied: &mut dyn FnMut(
            &mut Self,
            &mut ConstraintSetStorage<'db>,
            &mut L,
            &mut PathAssignments,
        ) -> ControlFlow<L::Break>,
    ) -> ControlFlow<L::Break> {
        // Don't bother adding the assignment and checking the sequent map if the edge takes us to
        // the ALWAYS_FALSE terminal.
        if child == ALWAYS_FALSE {
            return ControlFlow::Continue(());
        }

        path.walk_edge(
            db,
            env,
            storage,
            assignment,
            |storage, path, _new_range, found_conflict| {
                if !found_conflict {
                    self.visit_node_and_then(
                        db,
                        env,
                        storage,
                        limits,
                        path,
                        child,
                        process_satisfied,
                    )?;
                }
                ControlFlow::Continue(())
            },
        )
    }

    fn validate_satisfied_path<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        all_typevars: &Support,
    ) -> ControlFlow<L::Break> {
        let mut all_typevars = all_typevars.clone();
        let mut seen_typevars = Support::default();
        self.validate_upper_bound_typevar(
            db,
            env,
            storage,
            limits,
            path,
            &mut all_typevars,
            &mut seen_typevars,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn validate_upper_bound_typevar<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        all_typevars: &mut Support,
        seen_typevars: &mut Support,
    ) -> ControlFlow<L::Break> {
        while let Some(typevar) = all_typevars.pop() {
            seen_typevars.insert(typevar);
            let bound_typevar = storage.typevar_data(typevar);
            let bound_or_constraints = bound_typevar.typevar(db).bound_or_constraints(db, env);
            let Some(TypeVarBoundOrConstraints::UpperBound(bound)) = bound_or_constraints else {
                continue;
            };

            let (constraint, source_order) = Constraint::new_node_with_bounds(
                db,
                env,
                storage,
                bound_typevar,
                None,
                Some(ConstraintBound::Validity(bound)),
            );
            self.source_orders
                .extend(storage.calculate_source_orders(source_order));

            // If any typevars are mentioned in the upper bound, we have to validate them too.
            if let Some(upper_bound_support) = storage.node_support(constraint) {
                let new_typevars = upper_bound_support - &*seen_typevars;
                *all_typevars |= &new_typevars;
            }

            return self.visit_node_and_then(
                db,
                env,
                storage,
                limits,
                path,
                constraint,
                &mut move |this, storage, limits, path| {
                    this.validate_upper_bound_typevar(
                        db,
                        env,
                        storage,
                        limits,
                        path,
                        all_typevars,
                        seen_typevars,
                    )
                },
            );
        }

        self.found_satisfied_path(limits, path)
    }

    fn found_satisfied_path<L: SolutionLimits>(
        &mut self,
        limits: &mut L,
        path: &PathAssignments,
    ) -> ControlFlow<L::Break> {
        limits.satisfied_path()?;
        let mut path: Vec<_> = path
            .positive_constraints()
            .map(|(constraint, source_constraint)| {
                let source_order = self
                    .source_orders
                    .get_index_of(&source_constraint)
                    .expect("every TDD constraint should have a source order");
                (constraint, source_order)
            })
            .collect();
        // Sort the constraints in each path by their `source_order`s, to ensure that we construct
        // any unions or intersections in our type mappings in a stable order. Constraints might
        // come out of `PathAssignments` with identical `source_order`s, but if they do, those
        // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
        // retain that stable per-tie ordering.
        path.sort_by_key(|(_, source_order)| *source_order);
        self.sorted_paths.push(path);
        ControlFlow::Continue(())
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
        if let [single_path] = self.sorted_paths.as_slice()
            && single_path.is_empty()
        {
            return PathBounds::Unconstrained;
        }

        self.sorted_paths.sort_by(|path1, path2| {
            let source_orders1 = path1.iter().map(|(_, source_order)| *source_order);
            let source_orders2 = path2.iter().map(|(_, source_order)| *source_order);
            source_orders1.cmp(source_orders2)
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
                    let bounds = mappings.entry(typevar).or_default();
                    bounds.add_lower(db, env, lower);

                    if let Type::TypeVar(lower_bound_typevar) = lower.ty() {
                        let bounds = mappings.entry(lower_bound_typevar).or_default();
                        bounds.add_upper(db, env, lower.with_type(Type::TypeVar(typevar)));
                    }
                }

                if let Some(upper) = constraint.bounds.upper {
                    let bounds = mappings.entry(typevar).or_default();
                    bounds.add_upper(db, env, upper);

                    if let Type::TypeVar(upper_bound_typevar) = upper.ty() {
                        let bounds = mappings.entry(upper_bound_typevar).or_default();
                        bounds.add_lower(db, env, upper.with_type(Type::TypeVar(typevar)));
                    }
                }
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
