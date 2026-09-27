use std::marker::PhantomData;
use std::ops::{ControlFlow, Range};

use indexmap::map::Slice;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::types::constraints::paths::PathAssignments;
use crate::types::constraints::support::Support;
use crate::types::constraints::variables::{Constraint, ConstraintProvenance, UnsatisfiableBound};
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, CandidateSolution, CandidateSolutions, CandidateTypeVarSolution,
    CandidateTypeVarSolver, ConstraintAssignment, ConstraintId, ConstraintSetStorage, NodeId,
    SolutionLimits, SolutionValidity, SolutionViolation, SolutionViolationKind,
};
use crate::types::typevar::{TypeVarBoundOrConstraints, TypeVarConstraints, TypeVarSet};
use crate::types::{BoundTypeVarIdentity, BoundTypeVarInstance, Type, any_over_type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

type ProcessSatisfied<'a, 'db, L, B> = dyn FnMut(
        &mut SolutionWalker<'db>,
        &mut ConstraintSetStorage<'db>,
        &mut L,
        &mut PathAssignments,
    ) -> ControlFlow<B>
    + 'a;

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,
    inferable: TypeVarSet<'db>,
    declared_constraint_solutions: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,

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
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>, inferable: TypeVarSet<'db>) -> Self {
        Self {
            source_orders,
            inferable,
            declared_constraint_solutions: FxHashMap::default(),
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
                    let constrained = validations.constrained.as_slice();
                    let mut satisfied = false;
                    this.validate_satisfied_path(
                        db,
                        env,
                        storage,
                        limits,
                        path,
                        upper_bounds,
                        constrained,
                        &mut |this, storage, limits, path| {
                            if this.found_satisfied_path(db, env, storage, limits, path)? {
                                satisfied = true;
                            }
                            ControlFlow::Continue(())
                        },
                    )?;

                    // If this path is not satisfied, we want to identify which particular upper
                    // bounds or constraints were violated. To do that, we have to re-check this
                    // path against each one individually.
                    if !satisfied {
                        this.attribute_typevar_failures(
                            db,
                            env,
                            storage,
                            limits,
                            path,
                            upper_bounds,
                            constrained,
                        )?;
                    }

                    ControlFlow::Continue(())
                }
                None => this
                    .found_satisfied_path(db, env, storage, limits, path)
                    .map_continue(|_| ()),
            },
        )
    }

    /// Visit a BDD node and all of its descendants, invoking the `process_satisfied` callback for
    /// any satisfiable path that is discovered.
    #[expect(clippy::too_many_arguments)]
    fn visit_node_and_then<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        node: NodeId,
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
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
    fn visit_edge<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        assignment: ConstraintAssignment,
        child: NodeId,
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
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

    fn with_declared_constraint_solution<R>(
        &mut self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
        declared_constraint_solution: Type<'db>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let identity = bound_typevar.identity(db);
        self.declared_constraint_solutions
            .insert(identity, declared_constraint_solution);
        let result = f(self);
        self.declared_constraint_solutions.remove(&identity);
        result
    }

    #[expect(clippy::too_many_arguments)]
    fn visit_constraints_and_then<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        constraints: &[ConstraintId],
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
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

    fn candidate_evidence(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> Option<CandidateTypeVarSolution<'db>> {
        let mut evidence = CandidateTypeVarSolver::default();
        for (constraint, _) in path.positive_constraints() {
            let constraint = storage.constraint_data(constraint);
            if constraint.provides_bound_for(db, bound_typevar)
                && constraint.provenance() == ConstraintProvenance::Evidence
            {
                evidence.add_constraint(db, bound_typevar, constraint);
            }
        }
        evidence.finish(db, env, storage, bound_typevar)
    }

    fn evidence_satisfies_declared_constraint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        evidence: &CandidateTypeVarSolution<'db>,
        constrained_ty: Type<'db>,
    ) -> bool {
        let constraint_lower = constrained_ty.bottom_materialization(db, env);
        let constraint_upper = constrained_ty.top_materialization(db, env);
        let (when_lower, when_lower_source_order) = match evidence.evidence_lower {
            Some(lower) => storage.load(
                db,
                env,
                &lower.when_assignable_to_owned(db, env, constraint_upper, self.inferable),
            ),
            None => (ALWAYS_TRUE, None),
        };
        let (when_upper, when_upper_source_order) = evidence.upper.iter_evidence().fold(
            (ALWAYS_TRUE, None),
            |(when, when_source_order), upper| {
                let (when_upper, when_upper_source_order) = storage.load(
                    db,
                    env,
                    &constraint_lower.when_assignable_to_owned(db, env, upper, self.inferable),
                );
                let when = when.and(storage, when_upper);
                let when_source_order =
                    storage.ordered_source_order(when_source_order, when_upper_source_order);
                (when, when_source_order)
            },
        );
        let when = when_lower.and(storage, when_upper);
        let when_source_order =
            storage.ordered_source_order(when_lower_source_order, when_upper_source_order);
        !when.is_never_satisfied(db, env, storage, when_source_order)
    }

    /// Having found a satisfiable path in the BDD, validates that path against the declared upper
    /// bound (TODO and constraints) of all relevant typevars.
    #[expect(clippy::too_many_arguments)]
    fn validate_satisfied_path<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        upper_bounds: &Slice<BoundTypeVarInstance<'db>, UpperBound>,
        constrained: &Slice<BoundTypeVarInstance<'db>, Constrained<'db>>,
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
    ) -> ControlFlow<L::Break> {
        // We have a path that represents a valid solution to the constraint set. Check if the
        // solution satisfies all of the typevars' declared upper bounds (TODO and constraints).
        self.validate_upper_bound(
            db,
            env,
            storage,
            limits,
            path,
            upper_bounds,
            constrained,
            process_satisfied,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn validate_upper_bound<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        upper_bounds: &Slice<BoundTypeVarInstance<'db>, UpperBound>,
        constrained: &Slice<BoundTypeVarInstance<'db>, Constrained<'db>>,
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
    ) -> ControlFlow<L::Break> {
        let Some(((_, upper_bound), upper_bounds)) = upper_bounds.split_first() else {
            // We've checked all typevars that have an upper bound. Next check the typevars with
            // declared constraints.
            return self.validate_constrained(
                db,
                env,
                storage,
                limits,
                path,
                constrained,
                process_satisfied,
            );
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
                this.validate_upper_bound(
                    db,
                    env,
                    storage,
                    limits,
                    path,
                    upper_bounds,
                    constrained,
                    process_satisfied,
                )
            },
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn validate_constrained<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        constrained: &Slice<BoundTypeVarInstance<'db>, Constrained<'db>>,
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
    ) -> ControlFlow<L::Break> {
        let Some(((&bound_typevar, constrained_typevar), constrained)) = constrained.split_first()
        else {
            // We've checked all constrained typevars, and we now know that the candidate solution
            // is valid.
            return process_satisfied(self, storage, limits, path);
        };

        // Constrained typevars are more complex than bounded typevars, since they introduce a
        // disjunction; and because they are _equivalence_ bounds, not _upper_ bounds. As long as
        // the candidate solution satisfies _at least one_ of the declared constraints, the
        // solution is valid.
        //
        // Naively, that means we would just check each of the declared constraints separately,
        // adding the respective equivalence bound to the current path. Because the constraint
        // gives an equivalence bound, this will "tighten" the solution to be exactly the declared
        // constraint, as long as the solution satisfies that constraint. If more than one declared
        // constraint is valid, we first prune them with a "tightest constraint wins" heuristic. If
        // there are still multiple valid declared constraints, it would be the caller's
        // responsibility to decide whether to report that as an ambiguous solve, or to do
        // something useful with the different possible solutions.
        //
        // However, if the candidate solution maps this typevar to a dynamic type, or to another
        // typevar, and that solution satisfies more than one declared constraint, then we _don't_
        // want to report separate tightened solutions for each compatible constraint. Rather, we
        // want to report the dynamic type or typevar itself as the solution.

        // First see if we should return a "family" solution. If multiple declared constraints are
        // satisfied, _and_ the solution is either dynamic or another typevar, then we can consider
        // using the solution as-is, rather than trying to force it to be exactly equal to one of
        // those constraints. (We call this a "family" solution since it's a single solution that
        // satisfies a family of compatible declared constraints.)
        //
        // Note that a fixed caller typevar can only be preserved when its constraints are a subset
        // of this typevar's constraints. A bounded typevar may specialize below its bound, so it
        // must be promoted to an individual declared constraint instead.
        let Some(evidence) = Self::candidate_evidence(db, env, storage, path, bound_typevar) else {
            // If the evidence is not satisfiable, then we can return early; none of the
            // constraints can possibly be satisfied.
            return ControlFlow::Continue(());
        };
        let has_no_evidence = evidence.evidence_lower.is_none() && !evidence.upper.has_evidence();
        let is_preservable_typevar = |ty| {
            let Type::TypeVar(typevar) = ty else {
                return false;
            };
            typevar.is_inferable(db, self.inferable)
                || typevar
                    .typevar(db)
                    .constraints(db, env)
                    .is_some_and(|actual_constraints| {
                        actual_constraints.iter().all(|actual| {
                            constrained_typevar
                                .declared_constraints
                                .iter()
                                .any(|declared| {
                                    actual.is_equivalent_to(db, env, declared.constrained_ty)
                                })
                        })
                    })
        };
        let contains_preservable_typevar =
            |ty| any_over_type(db, env, ty, false, is_preservable_typevar);
        let has_bare_preservable_typevar_evidence =
            evidence.evidence_lower.is_some_and(is_preservable_typevar)
                || evidence
                    .as_single_upper_bound(db, env)
                    .is_some_and(is_preservable_typevar);
        let has_non_concrete_evidence = has_no_evidence
            || evidence.has_only_non_concrete_evidence == Some(true)
            || has_bare_preservable_typevar_evidence;

        if has_non_concrete_evidence {
            let has_preservable_typevar_evidence = evidence
                .evidence_lower
                .is_some_and(contains_preservable_typevar)
                || evidence
                    .as_single_upper_bound(db, env)
                    .is_some_and(contains_preservable_typevar);

            let mut potentially_satisfied_constraint_count = 0;
            for declared_constraint in &constrained_typevar.declared_constraints {
                let Some(constraints) = declared_constraint.constraints.as_deref() else {
                    continue;
                };

                let mut satisfied = false;
                self.visit_constraints_and_then(
                    db,
                    env,
                    storage,
                    limits,
                    path,
                    constraints,
                    &mut |_this, _storage, _limits, _path| {
                        // We don't need to use pending_candidate_solution here to verify that the
                        // solution is actually valid, because we can accept false positives. We
                        // will catch the failure when we fall through to the full family solution
                        // check below.
                        satisfied = true;
                        ControlFlow::Continue(())
                    },
                )?;

                if satisfied {
                    potentially_satisfied_constraint_count += 1;
                }
            }

            if potentially_satisfied_constraint_count > 1 {
                // We're eligible to return a family solution, but first we need to find it! First
                // check any remaining constrained typevars with _no_ validity assignment for this
                // typevar.
                let previously_pending = self.pending.len();
                let mut has_family_solution = false;
                let mut individual_solution_is_required = false;
                self.validate_constrained(
                    db,
                    env,
                    storage,
                    limits,
                    path,
                    constrained,
                    &mut |this, storage, limits, path| {
                        // Check which declared constraints are compatible with this complete
                        // solution for the remaining constrained typevars. Note that we _don't_
                        // update the candidate solution for those declared constraints — we want
                        // to return the family solution, after all. We just want to make sure that
                        // the individual declared constraints don't _invalidate_ that solution.
                        let mut satisfied_constraint_count = 0;
                        for declared_constraint in &constrained_typevar.declared_constraints {
                            let mut satisfied = false;
                            if let Some(constraints) = declared_constraint.constraints.as_deref() {
                                this.visit_constraints_and_then(
                                    db,
                                    env,
                                    storage,
                                    limits,
                                    path,
                                    constraints,
                                    &mut |this, storage, _limits, path| {
                                        if !has_preservable_typevar_evidence
                                            && !this.evidence_satisfies_declared_constraint(
                                                db,
                                                env,
                                                storage,
                                                &evidence,
                                                declared_constraint.constrained_ty,
                                            )
                                        {
                                            return ControlFlow::Continue(());
                                        }
                                        let solution = this.pending_candidate_solution(
                                            db, env, storage, path, None,
                                        );
                                        if solution.is_some() {
                                            satisfied = true;
                                        }
                                        ControlFlow::Continue(())
                                    },
                                )?;
                            }
                            if satisfied {
                                satisfied_constraint_count += 1;
                            }
                        }

                        match satisfied_constraint_count {
                            0 => {
                                // This family solution does not satisfy _any_ of the declared
                                // constraints. It definitely cannot be used as a solution, and
                                // also does not affect whether any other potential family
                                // solutions can be used.
                                ControlFlow::Continue(())
                            }
                            1 => {
                                // This family solution satisfies exactly one declared constraint.
                                // Family solutions are only used when the can consolidate more
                                // than one declared constraint. That means we don't want to use
                                // this family solution _or any other_. We'll create one or more
                                // individual solutions below.
                                individual_solution_is_required = true;
                                ControlFlow::Continue(())
                            }
                            _ => {
                                // This solution satisfies more than one declared constraint, so
                                // it's one of the eligible family solutions that we can report.
                                has_family_solution = true;
                                process_satisfied(this, storage, limits, path)
                            }
                        }
                    },
                )?;

                // If we found at least one valid family solution, we can go ahead and return them.
                // If any potential family solution only matched a single declared constraint, we
                // need to fall through and find individual solutions
                // If every valid assignment for the remaining typevars admitted a family
                // solution, there is no need to also record the individual constraints.
                if has_family_solution && !individual_solution_is_required {
                    return ControlFlow::Continue(());
                }

                // If any family solution only matched a single declared constraint; or if we
                // didn't find any family solutions at all, we have to fall through and look for
                // individual solutions. Before proceeding, we remove any potential family
                // solutions we might have found during our search.
                self.pending.truncate(previously_pending);
            }
        }

        // We cannot return only family solutions, so also check which individual declared
        // constraints can be used in the solution.
        let previously_pending = self.pending.len();
        let has_lower_bound_evidence = path.positive_constraints().any(|(constraint, _)| {
            let constraint = storage.constraint_data(constraint);
            constraint.provides_lower_bound_for(db, bound_typevar)
        });

        // A constraint preferred over every potentially valid alternative will also be preferred
        // over any subset of those alternatives. Try it first so that a successful branch can
        // discard the dominated alternatives before they multiply with later typevars.
        let preferred =
            constrained_typevar.preferred_constraint(db, env, has_lower_bound_evidence, |idx| {
                constrained_typevar.declared_constraints[idx]
                    .constraints
                    .is_some()
            });
        if let Some(preferred) = preferred
            && self.validate_single_declared_constraint(
                db,
                env,
                storage,
                limits,
                path,
                bound_typevar,
                &evidence,
                &constrained_typevar.declared_constraints[preferred],
                constrained,
                process_satisfied,
            )?
        {
            return ControlFlow::Continue(());
        }

        let constraint_count = constrained_typevar.declared_constraints.len();
        let mut constraint_satisfied = SmallVec::<[bool; 4]>::with_capacity(constraint_count);
        let mut constraint_solutions =
            SmallVec::<[Range<usize>; 4]>::with_capacity(constraint_count);
        for (idx, declared_constraint) in
            constrained_typevar.declared_constraints.iter().enumerate()
        {
            let start = self.pending.len();
            if preferred == Some(idx) {
                // We already checked this one above.
                constraint_satisfied.push(false);
                constraint_solutions.push(start..start);
                continue;
            }

            let satisfied = self.validate_single_declared_constraint(
                db,
                env,
                storage,
                limits,
                path,
                bound_typevar,
                &evidence,
                declared_constraint,
                constrained,
                process_satisfied,
            )?;
            let end = self.pending.len();
            constraint_satisfied.push(satisfied);
            constraint_solutions.push(start..end);
        }

        // Fast path: If exactly one constraint was satisfied, we can return its solutions
        // immediately. If _no_ constraints were satisfied, we can return its _lack_ of solutions
        // immediately.
        let satisfied_constraint_count = constraint_satisfied
            .iter()
            .filter(|satisfied| **satisfied)
            .count();
        if satisfied_constraint_count <= 1 {
            return ControlFlow::Continue(());
        }

        // At this point, we know that more than one constraint was satisfied. Check to see if any
        // one of them is preferred over all of the others. If so, we prefer that single solution.
        let preferred =
            constrained_typevar.preferred_constraint(db, env, has_lower_bound_evidence, |idx| {
                constraint_satisfied[idx]
            });

        // If there was a single preferred constraint, remove the solutions from the other
        // constraints. Otherwise keep them all, and let the caller decide how to handle the
        // ambiguity.
        if let Some(best) = preferred {
            let solutions = &constraint_solutions[best];
            self.pending.truncate(solutions.end);
            self.pending.drain(previously_pending..solutions.start);
        }

        ControlFlow::Continue(())
    }

    #[expect(clippy::too_many_arguments)]
    fn validate_single_declared_constraint<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        bound_typevar: BoundTypeVarInstance<'db>,
        evidence: &CandidateTypeVarSolution<'db>,
        declared_constraint: &DeclaredConstraint<'db>,
        constrained: &Slice<BoundTypeVarInstance<'db>, Constrained<'db>>,
        process_satisfied: &mut ProcessSatisfied<'_, 'db, L, L::Break>,
    ) -> ControlFlow<L::Break, bool> {
        let mut satisfied = false;
        if let Some(constraints) = declared_constraint.constraints.as_deref() {
            self.with_declared_constraint_solution(
                db,
                bound_typevar,
                declared_constraint.constrained_ty,
                |this| {
                    this.visit_constraints_and_then(
                        db,
                        env,
                        storage,
                        limits,
                        path,
                        constraints,
                        &mut |this, storage, limits, path| {
                            // Selecting a concrete constraint must not specialize a caller's fixed
                            // typevar: `S & str <= int` may hold for some `S`, but not for every `S`.
                            if !this.evidence_satisfies_declared_constraint(
                                db,
                                env,
                                storage,
                                evidence,
                                declared_constraint.constrained_ty,
                            ) {
                                return ControlFlow::Continue(());
                            }

                            // The candidate solution satisfies this declared constraint, but we still
                            // need to check any remaining constrained typevars.
                            this.validate_constrained(
                                db,
                                env,
                                storage,
                                limits,
                                path,
                                constrained,
                                &mut |this, storage, limits, path| {
                                    satisfied = true;
                                    process_satisfied(this, storage, limits, path)
                                },
                            )
                        },
                    )
                },
            )?;
        }
        ControlFlow::Continue(satisfied)
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
        typevar_violations: Option<&FxHashMap<BoundTypeVarInstance<'db>, SolutionViolationKind>>,
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
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, CandidateTypeVarSolver<'db>> =
            FxIndexMap::default();

        for (constraint, _) in typevars {
            let constraint = storage.constraint_data(constraint);
            match constraint {
                Constraint::ConcreteLower(lower) => {
                    let solver = mappings.entry(lower.typevar).or_default();
                    solver.add_constraint(db, lower.typevar, constraint);
                }
                Constraint::ConcreteUpper(upper) => {
                    let solver = mappings.entry(upper.typevar).or_default();
                    solver.add_constraint(db, upper.typevar, constraint);
                }
                Constraint::ConcreteEquivalence(equivalence) => {
                    let solver = mappings.entry(equivalence.typevar).or_default();
                    solver.add_constraint(db, equivalence.typevar, constraint);
                }
                Constraint::TypeVarRange(bound) => {
                    let solver = mappings.entry(bound.left).or_default();
                    solver.add_constraint(db, bound.left, constraint);
                    let solver = mappings.entry(bound.right).or_default();
                    solver.add_constraint(db, bound.right, constraint);
                }
                Constraint::TypeVarEquivalence(bound) => {
                    let (left, right) = bound.in_builder(db, storage);
                    let solver = mappings.entry(left).or_default();
                    solver.add_constraint(db, left, constraint);
                    let solver = mappings.entry(right).or_default();
                    solver.add_constraint(db, right, constraint);
                }
            }
        }

        let mut violations = Vec::new();
        let typevars: Option<Box<[_]>> = mappings
            .into_iter()
            .map(|(bound_typevar, solver)| {
                let mut solution = solver.finish(db, env, storage, bound_typevar)?;
                let argument = match self
                    .declared_constraint_solutions
                    .get(&bound_typevar.identity(db))
                {
                    Some(&ty) => {
                        solution.selected_declared_constraint = Some(ty);
                        Some(ty)
                    }
                    None => solution.inference_lower(db, env),
                };

                if let Some(typevar_violations) = typevar_violations
                    && let Some(&kind) = typevar_violations.get(&bound_typevar)
                {
                    violations.push(SolutionViolation {
                        bound_typevar,
                        argument,
                        variance: solution.variance(),
                        kind,
                    });
                }

                Some(solution)
            })
            .collect();
        let typevars = typevars?;

        let validity = match typevar_violations {
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
    ) -> ControlFlow<L::Break, bool> {
        let Some(pending) = self.pending_candidate_solution(db, env, storage, path, None) else {
            return ControlFlow::Continue(false);
        };
        limits.satisfied_path()?;
        self.pending.push(pending);
        ControlFlow::Continue(true)
    }

    /// Having already determined that a satisfiable path violates the declared upper bounds (TODO
    /// and constraints) of the relevant typevars, determines _which particular_ upper bounds or
    /// constraints were violated. Adds an [`Invalid`][SolutionValidity::Invalid] candidate
    /// solution for the path recording those violations, so that a later stage can transform them
    /// into useful diagnostics.
    #[expect(clippy::too_many_arguments)]
    fn attribute_typevar_failures<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        limits: &mut L,
        path: &mut PathAssignments,
        upper_bounds: &Slice<BoundTypeVarInstance<'db>, UpperBound>,
        constrained: &Slice<BoundTypeVarInstance<'db>, Constrained<'db>>,
    ) -> ControlFlow<L::Break> {
        let mut violations = FxHashMap::default();

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
                violations.insert(*bound_typevar, SolutionViolationKind::UpperBound);
            }
        }

        for (bound_typevar, constrained_typevar) in constrained {
            let Some(evidence) = Self::candidate_evidence(db, env, storage, path, *bound_typevar)
            else {
                violations.insert(*bound_typevar, SolutionViolationKind::Constraints);
                continue;
            };

            let mut satisfied = false;
            for declared_constraint in &constrained_typevar.declared_constraints {
                if let Some(constraints) = declared_constraint.constraints.as_deref() {
                    self.visit_constraints_and_then(
                        db,
                        env,
                        storage,
                        limits,
                        path,
                        constraints,
                        &mut |this, storage, _limits, path| {
                            if !this.evidence_satisfies_declared_constraint(
                                db,
                                env,
                                storage,
                                &evidence,
                                declared_constraint.constrained_ty,
                            ) {
                                return ControlFlow::Continue(());
                            }
                            let pending =
                                this.pending_candidate_solution(db, env, storage, path, None);
                            if pending.is_some() {
                                satisfied = true;
                            }
                            ControlFlow::Continue(())
                        },
                    )?;
                }
            }
            if !satisfied {
                violations.insert(*bound_typevar, SolutionViolationKind::Constraints);
            }
        }

        // Complete validation failed, but no single declaration explains why. The declarations
        // are only inconsistent in combination, so there is no attributable candidate to retain.
        if violations.is_empty() {
            return ControlFlow::Continue(());
        }

        if let Some(pending) =
            self.pending_candidate_solution(db, env, storage, path, Some(&violations))
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
    constrained: FxIndexMap<BoundTypeVarInstance<'db>, Constrained<'db>>,
}

type ValidationConstraints = Option<SmallVec<[ConstraintId; 4]>>;

struct UpperBound {
    constraints: ValidationConstraints,
}

struct Constrained<'db> {
    declared_constraints: SmallVec<[DeclaredConstraint<'db>; 4]>,
}

struct DeclaredConstraint<'db> {
    constraints: ValidationConstraints,
    constrained_ty: Type<'db>,
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
            Some(TypeVarBoundOrConstraints::Constraints(declared_constraints)) => self
                .add_constrained(
                    db,
                    env,
                    storage,
                    typevar_queue,
                    seen_typevars,
                    bound_typevar,
                    declared_constraints,
                ),
            None => {}
        }
    }

    fn intern_typevar_constraints(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar_queue: &mut Support,
        seen_typevars: &mut Support,
        constraints: impl Iterator<Item = Result<Constraint<'db>, UnsatisfiableBound>>,
    ) -> ValidationConstraints {
        let constraints: ValidationConstraints = constraints
            .map(Result::ok)
            .map(|constraint| {
                constraint.map(|constraint| storage.intern_constraint(db, env, constraint))
            })
            .collect();

        // If any typevars are mentioned in the upper bound, we have to validate them too.
        // TODO: Consider calculating this at construction time, so that here we have a fixed
        // set of typevars to check.
        for constraint in constraints.iter().flatten() {
            let constraint_support = storage.constraint_support(*constraint);
            let new_typevars = constraint_support - &*seen_typevars;
            *typevar_queue |= &new_typevars;
        }

        constraints
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
            let constraints = Self::intern_typevar_constraints(
                db,
                env,
                storage,
                typevar_queue,
                seen_typevars,
                constraints,
            );
            UpperBound { constraints }
        });
    }

    #[expect(clippy::too_many_arguments)]
    fn add_constrained(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar_queue: &mut Support,
        seen_typevars: &mut Support,
        bound_typevar: BoundTypeVarInstance<'db>,
        declared_constraints: TypeVarConstraints<'db>,
    ) {
        self.constrained.entry(bound_typevar).or_insert_with(|| {
            let declared_constraints = declared_constraints
                .elements(db)
                .iter()
                .map(|&constrained_ty| {
                    let constraints = Constraint::new_equivalence_bound(
                        db,
                        env,
                        ConstraintProvenance::Validity,
                        bound_typevar,
                        constrained_ty,
                    );
                    let constraints = Self::intern_typevar_constraints(
                        db,
                        env,
                        storage,
                        typevar_queue,
                        seen_typevars,
                        constraints,
                    );
                    DeclaredConstraint {
                        constraints,
                        constrained_ty,
                    }
                })
                .collect();
            Constrained {
                declared_constraints,
            }
        });
    }
}

impl<'db> Constrained<'db> {
    fn preferred_constraint(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        has_lower_bound_evidence: bool,
        is_eligible: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        'candidate: for (candidate_idx, declared_constraint) in
            self.declared_constraints.iter().enumerate()
        {
            if !is_eligible(candidate_idx) {
                continue;
            }

            let candidate = declared_constraint.constrained_ty;
            for (other_idx, other_constraint) in self.declared_constraints.iter().enumerate() {
                if candidate_idx == other_idx || !is_eligible(other_idx) {
                    continue;
                }

                let other = other_constraint.constrained_ty;
                let candidate_assignable_to_other = candidate.is_assignable_to(db, env, other);
                let other_assignable_to_candidate = other.is_assignable_to(db, env, candidate);

                // Lower-bound evidence asks for the narrowest compatible declared constraint
                // above the lower bound. With only upper-bound evidence, ask for the widest
                // compatible declared constraint below the upper bound. If the candidates are
                // assignable in both directions, prefer a fully static constraint over a gradual
                // one. Equivalent constraints preserve declaration order.
                let candidate_is_at_least_as_good =
                    match (candidate_assignable_to_other, other_assignable_to_candidate) {
                        (false, false) => false,
                        (true, false) => has_lower_bound_evidence,
                        (false, true) => !has_lower_bound_evidence,
                        (true, true) => {
                            candidate.is_fully_static(db, env) || !other.is_fully_static(db, env)
                        }
                    };
                if !candidate_is_at_least_as_good {
                    continue 'candidate;
                }
            }

            return Some(candidate_idx);
        }
        None
    }
}
