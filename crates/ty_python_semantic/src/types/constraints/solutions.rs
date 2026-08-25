use std::marker::PhantomData;
use std::ops::ControlFlow;

use rustc_hash::FxHashSet;

use crate::types::constraints::paths::PathAssignments;
use crate::types::constraints::support::Support;
use crate::types::constraints::variables::{Constraint, ConstraintProvenance};
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, CandidateSolution, CandidateSolutions, ConstraintId,
    ConstraintSetStorage, NodeId, PathBoundBuilder, SolutionLimits, SolutionValidity,
    SolutionViolation, SolutionViolationKind,
};
use crate::types::typevar::TypeVarBoundOrConstraints;
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,
    pending: Vec<PendingCandidateSolution<'db>>,
    upper_bounds: Vec<(BoundTypeVarInstance<'db>, NodeId)>,
    _phantom: PhantomData<&'db ()>,
}

struct PendingCandidateSolution<'db> {
    candidate: CandidateSolution<'db>,
    source_orders: Vec<usize>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>) -> Self {
        Self {
            source_orders,
            pending: Vec::default(),
            upper_bounds: Vec::default(),
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
                None => this.found_satisfied_path(db, env, storage, limits, path),
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
            )?;
        }
        ControlFlow::Continue(())
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
        // We have a path that represents a valid solution to the constraint set. First verify that
        // solution satisfies all of the typevars' declared upper bounds (TODO and constraints).
        let mut all_typevars = all_typevars.clone();
        let mut seen_typevars = Support::default();
        let previous_count = self.pending.len();
        self.upper_bounds.clear();
        self.validate_upper_bound_typevar(
            db,
            env,
            storage,
            limits,
            path,
            &mut all_typevars,
            &mut seen_typevars,
        )?;
        if self.pending.len() > previous_count {
            // There is at least one extension of the valid solution that satisfies all declared
            // upper bounds (TODO and constraints), and we've already recorded pending solutions
            // for them. Nothing more to do.
            return ControlFlow::Continue(());
        }

        // To see if the solution is actually valid, we checked against _all_ declared upper bounds
        // (TODO and constraints). Now we have to re-check them _individually_ to create better
        // diagnostics.
        self.attribute_typevar_failures(db, env, storage, limits, path)
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

            let constraints = Constraint::new_upper_bound(
                db,
                env,
                ConstraintProvenance::Validity,
                bound_typevar,
                bound,
            );
            let (constraint, source_order) = Constraint::new_nodes(db, env, storage, constraints);
            self.source_orders
                .extend(storage.calculate_source_orders(source_order));
            self.upper_bounds.push((bound_typevar, constraint));

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
                &mut |this, storage, limits, path| {
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

        self.found_satisfied_path(db, env, storage, limits, path)
    }

    /// Create a pending candidate solution for the current path.
    ///
    /// This method is fallible because a path to the `true` terminal might still be unsatisfiable,
    /// if it introduces conflicting bounds for a typevar. (The sequent map _should_ detect most
    /// cases of conflicting bounds, but we have some final last-minute checks here to catch cases
    /// that the sequent map can't handle yet.)
    ///
    /// TODO(dcreager): I consider this a bug in the sequent map, which should be addressed in its
    /// own right, since there are many other methods that assume that a path to `true` terminal
    /// indicates satisfiability.
    fn pending_candidate_solution(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
        upper_bound_violations: Option<&FxHashSet<BoundTypeVarInstance<'db>>>,
    ) -> Option<PendingCandidateSolution<'db>> {
        // Sort the constraints in each path by their `source_order`s, to ensure that we construct
        // any unions or intersections in our type mappings in a stable order. Constraints might
        // come out of `PathAssignments` with identical `source_order`s, but if they do, those
        // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
        // retain that stable per-tie ordering.
        let mut typevars: Vec<_> = path
            .positive_constraints()
            .map(|(constraint, source_constraint)| {
                let source_order = self
                    .source_orders
                    .get_index_of(&source_constraint)
                    .expect("every TDD constraint should have a source order");
                (constraint, source_order)
            })
            .collect();
        typevars.sort_by_key(|(_, source_order)| *source_order);
        let source_orders = typevars
            .iter()
            .map(|(_, source_order)| *source_order)
            .collect();

        // Then collect the combined lower and upper bounds for each typevar.
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>> =
            FxIndexMap::default();

        for (constraint, _) in typevars {
            let constraint = storage.constraint_data(constraint);
            match constraint {
                Constraint::ConcreteLower(lower) => {
                    let bounds = mappings.entry(lower.typevar).or_default();
                    bounds.add_lower(db, env, lower.provenance, lower.bound);
                }
                Constraint::ConcreteUpper(upper) => {
                    let bounds = mappings.entry(upper.typevar).or_default();
                    bounds.add_upper(db, env, upper.provenance, upper.bound);
                }
                Constraint::ConcreteEquivalence(equivalence) => {
                    let bounds = mappings.entry(equivalence.typevar).or_default();
                    bounds.add_lower(db, env, equivalence.provenance, equivalence.bound);
                    bounds.add_upper(db, env, equivalence.provenance, equivalence.bound);
                }
                Constraint::TypeVarRange(bound) => {
                    let bounds = mappings.entry(bound.left).or_default();
                    bounds.add_upper(db, env, bound.provenance, Type::TypeVar(bound.right));
                    let bounds = mappings.entry(bound.right).or_default();
                    bounds.add_lower(db, env, bound.provenance, Type::TypeVar(bound.left));
                }
                Constraint::TypeVarEquivalence(bound) => {
                    let (left, right) = bound.in_builder(db, storage);
                    let bounds = mappings.entry(left).or_default();
                    bounds.add_lower(db, env, bound.provenance, Type::TypeVar(right));
                    bounds.add_upper(db, env, bound.provenance, Type::TypeVar(right));
                    let bounds = mappings.entry(right).or_default();
                    bounds.add_lower(db, env, bound.provenance, Type::TypeVar(left));
                    bounds.add_upper(db, env, bound.provenance, Type::TypeVar(left));
                }
            }
        }

        let mut violations = Vec::new();
        let typevars: Option<Box<[_]>> = mappings
            .into_iter()
            .map(|(bound_typevar, bounds)| {
                let path_bound = bounds.finish(db, env, bound_typevar);

                let lower = path_bound.effective_lower(db, env);
                if !path_bound.upper.is_satisfied_by(db, env, lower) {
                    let (when_upper, source_order) =
                        path_bound.upper.when_satisfied_by(db, env, storage, lower);
                    if when_upper.is_never_satisfied(db, env, storage, source_order) {
                        // This path does not satisfy the accumulated upper bound, and is
                        // therefore not a valid specialization.
                        return None;
                    }
                }

                if let Some(upper_bound_violations) = upper_bound_violations
                    && upper_bound_violations.contains(&bound_typevar)
                {
                    violations.push(SolutionViolation {
                        bound_typevar,
                        argument: path_bound.evidence_lower(),
                        variance: path_bound.variance(),
                        kind: SolutionViolationKind::UpperBound,
                    });
                }

                Some(path_bound)
            })
            .collect();
        let typevars = typevars?;

        let validity = match upper_bound_violations {
            None => SolutionValidity::Valid,
            Some(_) if violations.is_empty() => return None,
            Some(_) => SolutionValidity::Invalid(violations.into_boxed_slice()),
        };
        let candidate = CandidateSolution { typevars, validity };
        let pending = PendingCandidateSolution {
            candidate,
            source_orders,
        };
        Some(pending)
    }

    fn found_satisfied_path<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &PathAssignments,
    ) -> ControlFlow<L::Break> {
        if let Some(pending) = self.pending_candidate_solution(db, env, storage, path, None) {
            limits.satisfied_path()?;
            self.pending.push(pending);
        }
        ControlFlow::Continue(())
    }

    fn attribute_typevar_failures<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
    ) -> ControlFlow<L::Break> {
        let mut upper_bound_violations = FxHashSet::default();
        let upper_bounds = std::mem::take(&mut self.upper_bounds);
        for (bound_typevar, constraint) in upper_bounds {
            let mut satisfied = false;
            self.visit_node_and_then(
                db,
                env,
                storage,
                limits,
                path,
                constraint,
                &mut |this, storage, _limits, path| {
                    let pending = this.pending_candidate_solution(db, env, storage, path, None);
                    if pending.is_some() {
                        satisfied = true;
                    }
                    ControlFlow::Continue(())
                },
            )?;
            if !satisfied {
                upper_bound_violations.insert(bound_typevar);
            }
        }

        // Complete validation failed, but no single declaration explains why. The declarations
        // are only inconsistent in combination, so there is no attributable candidate to retain.
        if upper_bound_violations.is_empty() {
            return ControlFlow::Continue(());
        }

        if let Some(pending) =
            self.pending_candidate_solution(db, env, storage, path, Some(&upper_bound_violations))
        {
            self.pending.push(pending);
        }
        ControlFlow::Continue(())
    }

    pub(super) fn finish(mut self) -> CandidateSolutions<'db> {
        if self.pending.is_empty() {
            return CandidateSolutions::Unsatisfiable;
        }
        if let [single] = self.pending.as_slice()
            && single.candidate.typevars.is_empty()
        {
            return CandidateSolutions::Unconstrained;
        }

        self.pending.sort_by(|pending1, pending2| {
            let source_orders1 = pending1.source_orders.iter().copied();
            let source_orders2 = pending2.source_orders.iter().copied();
            source_orders1.cmp(source_orders2)
        });

        let result = self
            .pending
            .drain(..)
            .map(|pending| pending.candidate)
            .collect();
        CandidateSolutions::Constrained(result)
    }
}
