use std::marker::PhantomData;
use std::ops::ControlFlow;

use indexmap::map::Slice;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::types::constraints::paths::PathAssignments;
use crate::types::constraints::support::Support;
use crate::types::constraints::variables::{Constraint, ConstraintProvenance};
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, CandidateSolution, CandidateSolutions, CandidateTypeVarRangeSolver,
    CandidateTypeVarSolution, ConstraintAssignment, ConstraintId, ConstraintSetStorage, NodeId,
    SolutionLimits, SolutionValidity, SolutionViolation, SolutionViolationKind,
};
use crate::types::typevar::TypeVarBoundOrConstraints;
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,

    /// Candidate solutions for each satisfiable path in the BDD.
    ///
    /// We will check these solutions against the declared upper bounds (TODO and constraints) of
    /// all relevant typevars (both inferable and non-inferable). Note that we will still create a
    /// candidate solution for satisfiable paths that do _not_ satisfy the upper bounds and
    /// constraints. Those paths will have a [`validity`][CandidateSolution::validity] of
    /// [`Invalid`][SolutionValidity::Invalid].
    pending: Vec<PendingCandidateSolution<'db>>,

    _phantom: PhantomData<&'db ()>,
}

struct PendingCandidateSolution<'db> {
    /// The candidate solution for a satisfiable path in the BDD
    candidate: CandidateSolution<'db>,

    /// The `source_orders` of the constraints in the path that this candidate solution was created
    /// from. We retain this so that our final result is sorted in a stable order.
    source_orders: Vec<usize>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>) -> Self {
        Self {
            source_orders,
            pending: Vec::default(),
            _phantom: PhantomData,
        }
    }

    /// Visit a BDD node and all of its descendants. We will add pending candidate solutions for
    /// any satisfiable path we discover from the node.
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
        let mut validations = None;
        self.visit_node_and_then(
            db,
            env,
            storage,
            limits,
            path,
            node,
            &mut |this, storage, limits, path| match all_typevars {
                Some(all_typevars) => {
                    let validations = validations.get_or_insert_with(|| {
                        Validations::from_support(db, env, storage, all_typevars)
                    });
                    let upper_bounds = validations.upper_bounds.as_slice();
                    this.validate_satisfied_path(db, env, storage, limits, path, upper_bounds)
                }
                None => this.found_satisfied_path(db, env, storage, limits, path),
            },
        )
    }

    /// Visit a BDD node and all of its descendants, invoking the `process_satisfied` callback for
    /// any satisfiable path that is discovered.
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
            self.visit_edge(
                db,
                env,
                storage,
                limits,
                path,
                assignment,
                child,
                process_satisfied,
            )?;
        }
        ControlFlow::Continue(())
    }

    /// Visits one of the outgoing edges from a BDD node.
    ///
    /// (This is a helper method used by [`visit_node_and_then`][Self::visit_node_and_then]. You
    /// will probably not need to call this directly.)
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

    #[expect(clippy::too_many_arguments)]
    #[expect(clippy::type_complexity)]
    fn visit_constraints_and_then<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        constraints: &[ConstraintId],
        process_satisfied: &mut dyn FnMut(
            &mut Self,
            &mut ConstraintSetStorage<'db>,
            &mut L,
            &mut PathAssignments,
        ) -> ControlFlow<L::Break>,
    ) -> ControlFlow<L::Break> {
        let Some((constraint, constraints)) = constraints.split_first() else {
            return process_satisfied(self, storage, limits, path);
        };
        self.source_orders.insert(*constraint);
        path.walk_edge(
            db,
            env,
            storage,
            constraint.when_true(),
            |storage, path, _new_range, found_conflict| {
                if !found_conflict {
                    self.visit_constraints_and_then(
                        db,
                        env,
                        storage,
                        limits,
                        path,
                        constraints,
                        process_satisfied,
                    )?;
                }
                ControlFlow::Continue(())
            },
        )
    }

    /// Having found a satisfiable path in the BDD, validates that path against the declared upper
    /// bound (TODO and constraints) of all relevant typevars.
    fn validate_satisfied_path<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        upper_bounds: &Slice<BoundTypeVarInstance<'db>, UpperBound>,
    ) -> ControlFlow<L::Break> {
        // We have a path that represents a valid solution to the constraint set. Check if the
        // solution satisfies all of the typevars' declared upper bounds (TODO and constraints).
        let previous_count = self.pending.len();
        self.validate_upper_bound(db, env, storage, limits, path, upper_bounds)?;
        if self.pending.len() > previous_count {
            // We will only add pending candidate solutions during the validation process if _all_
            // validations
            // If we added any pending candidate solutions during the validation process, then the
            // solution is valid!
            return ControlFlow::Continue(());
        }

        // If we fall through, then the solution did not satisfy all of the declared upper bounds
        // (TODO and constraints). If we can, we want to identify which particular upper bounds or
        // constraints were violated. To do that, we have to re-check this path against each one
        // individually.
        self.attribute_typevar_failures(db, env, storage, limits, path, upper_bounds)
    }

    fn validate_upper_bound<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        upper_bounds: &Slice<BoundTypeVarInstance<'db>, UpperBound>,
    ) -> ControlFlow<L::Break> {
        let Some(((_, upper_bound), upper_bounds)) = upper_bounds.split_first() else {
            // We've checked all typevars that have an upper bound, and we now know that the
            // candidate solution is valid.
            // TODO: Check the declared constraints here instead of `preliminary_solve` before
            // declaring the candidate solution valid.
            return self.found_satisfied_path(db, env, storage, limits, path);
        };

        let Some(constraints) = upper_bound.constraints.as_deref() else {
            // This upper bound is entirely unsatisfiable.
            return ControlFlow::Continue(());
        };

        // Verify that we can add all of the upper bound's constraints to the current path without
        // making it unsatisfiable. If we can, make a recursive call to check the next typevar with
        // an upper bound.
        self.visit_constraints_and_then(
            db,
            env,
            storage,
            limits,
            path,
            constraints,
            &mut |this, storage, limits, path| {
                this.validate_upper_bound(db, env, storage, limits, path, upper_bounds)
            },
        )
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
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, CandidateTypeVarRangeSolver<'db>> =
            FxIndexMap::default();

        for (constraint, _) in typevars {
            let constraint = storage.constraint_data(constraint);
            match constraint {
                Constraint::ConcreteLower(lower) => {
                    let solver = mappings.entry(lower.typevar).or_default();
                    solver.add_lower(lower.provenance, lower.bound);
                }
                Constraint::ConcreteUpper(upper) => {
                    let solver = mappings.entry(upper.typevar).or_default();
                    solver.add_upper(upper.provenance, upper.bound);
                }
                Constraint::ConcreteEquivalence(equivalence) => {
                    let solver = mappings.entry(equivalence.typevar).or_default();
                    solver.add_lower(equivalence.provenance, equivalence.bound);
                    solver.add_upper(equivalence.provenance, equivalence.bound);
                }
                Constraint::TypeVarRange(bound) => {
                    let solver = mappings.entry(bound.left).or_default();
                    solver.add_upper(bound.provenance, Type::TypeVar(bound.right));
                    let solver = mappings.entry(bound.right).or_default();
                    solver.add_lower(bound.provenance, Type::TypeVar(bound.left));
                }
                Constraint::TypeVarEquivalence(bound) => {
                    let (left, right) = bound.in_builder(db, storage);
                    let solver = mappings.entry(left).or_default();
                    solver.add_lower(bound.provenance, Type::TypeVar(right));
                    solver.add_upper(bound.provenance, Type::TypeVar(right));
                    let solver = mappings.entry(right).or_default();
                    solver.add_lower(bound.provenance, Type::TypeVar(left));
                    solver.add_upper(bound.provenance, Type::TypeVar(left));
                }
            }
        }

        let mut violations = Vec::new();
        let typevars: Option<Box<[_]>> = mappings
            .into_iter()
            .map(|(bound_typevar, solver)| {
                let range = solver.finish(db, env, storage, bound_typevar)?;

                if let Some(upper_bound_violations) = upper_bound_violations
                    && upper_bound_violations.contains(&bound_typevar)
                {
                    violations.push(SolutionViolation {
                        bound_typevar,
                        argument: range.inference_lower(db, env),
                        variance: range.variance(),
                        kind: SolutionViolationKind::UpperBound,
                    });
                }

                Some(CandidateTypeVarSolution::range(bound_typevar, range))
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

    /// Having already determined that a satisfiable path violates the declared upper bounds (TODO
    /// and constraints) of the relevant typevars, determines _which particular_ upper bounds or
    /// constraints were violated. Adds an [`Invalid`][SolutionValidity::Invalid] candidate
    /// solution for the path recording those violations, so that a later stage can transform them
    /// into useful diagnostics.
    fn attribute_typevar_failures<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        upper_bounds: &Slice<BoundTypeVarInstance<'db>, UpperBound>,
    ) -> ControlFlow<L::Break> {
        let mut upper_bound_violations = FxHashSet::default();
        for (bound_typevar, upper_bound) in upper_bounds {
            let mut satisfied = false;
            if let Some(constraints) = upper_bound.constraints.as_deref() {
                self.visit_constraints_and_then(
                    db,
                    env,
                    storage,
                    limits,
                    path,
                    constraints,
                    &mut |this, storage, _limits, path| {
                        let pending = this.pending_candidate_solution(db, env, storage, path, None);
                        if pending.is_some() {
                            satisfied = true;
                        }
                        ControlFlow::Continue(())
                    },
                )?;
            }
            if !satisfied {
                upper_bound_violations.insert(*bound_typevar);
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

/// Validations that must be verified for each candidate solution.
#[derive(Default)]
struct Validations<'db> {
    upper_bounds: FxIndexMap<BoundTypeVarInstance<'db>, UpperBound>,
}

struct UpperBound {
    constraints: Option<SmallVec<[ConstraintId; 4]>>,
}

impl<'db> Validations<'db> {
    fn from_support(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        all_typevars: &Support,
    ) -> Self {
        let mut result = Self::default();
        let mut typevar_queue = all_typevars.clone();
        let mut seen_typevars = Support::default();
        while let Some(typevar) = typevar_queue.pop() {
            seen_typevars.insert(typevar);
            let bound_typevar = storage.typevar_data(typevar);
            result.add_typevar(
                db,
                env,
                storage,
                &mut typevar_queue,
                &mut seen_typevars,
                bound_typevar,
            );
        }
        result
    }

    fn add_typevar(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar_queue: &mut Support,
        seen_typevars: &mut Support,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) {
        let bound_or_constraints = bound_typevar.typevar(db).bound_or_constraints(db, env);
        match bound_or_constraints {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => self.add_upper_bound(
                db,
                env,
                storage,
                typevar_queue,
                seen_typevars,
                bound_typevar,
                bound,
            ),
            Some(TypeVarBoundOrConstraints::Constraints(_)) => {
                // TODO
            }
            None => {}
        }
    }

    #[expect(clippy::too_many_arguments)]
    fn add_upper_bound(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar_queue: &mut Support,
        seen_typevars: &mut Support,
        bound_typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) {
        self.upper_bounds.entry(bound_typevar).or_insert_with(|| {
            let constraints = Constraint::new_upper_bound(
                db,
                env,
                ConstraintProvenance::Validity,
                bound_typevar,
                bound,
            );
            let constraints = constraints
                .map(Result::ok)
                .map(|constraint| {
                    constraint.map(|constraint| storage.intern_constraint(db, env, constraint))
                })
                .collect();
            let upper_bound = UpperBound { constraints };

            // If any typevars are mentioned in the upper bound, we have to validate them too.
            // TODO: Consider calculating this at construction time, so that here we have a fixed
            // set of typevars to check.
            for constraint in upper_bound.constraints.iter().flatten() {
                let constraint_support = storage.constraint_support(*constraint);
                let new_typevars = constraint_support - &*seen_typevars;
                *typevar_queue |= &new_typevars;
            }

            upper_bound
        });
    }
}
