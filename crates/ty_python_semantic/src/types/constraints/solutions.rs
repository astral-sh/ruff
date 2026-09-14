use std::marker::PhantomData;
use std::ops::ControlFlow;

use crate::types::constraints::paths::PathAssignments;
use crate::types::constraints::variables::Constraint;
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, CandidateSolution, CandidateSolutions, ConstraintId,
    ConstraintSetStorage, NodeId, PathBoundBuilder, SolutionLimits, SolutionValidity,
};
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,
    pending: Vec<PendingCandidateSolution>,
    _phantom: PhantomData<&'db ()>,
}

struct PendingCandidateSolution {
    typevars: Vec<(ConstraintId, usize)>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>) -> Self {
        Self {
            source_orders,
            pending: Vec::default(),
            _phantom: PhantomData,
        }
    }

    pub(super) fn visit_node<L: SolutionLimits>(
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

        // If the current node is ALWAYS_TRUE, we can immediately report the current solution.
        if node == ALWAYS_TRUE {
            limits.satisfied_path()?;
            self.found_satisfied_path(path);
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

    fn found_satisfied_path(&mut self, path: &PathAssignments) {
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
        // Sort the constraints in each path by their `source_order`s, to ensure that we construct
        // any unions or intersections in our type mappings in a stable order. Constraints might
        // come out of `PathAssignments` with identical `source_order`s, but if they do, those
        // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
        // retain that stable per-tie ordering.
        typevars.sort_by_key(|(_, source_order)| *source_order);
        let pending = PendingCandidateSolution { typevars };
        self.pending.push(pending);
    }

    pub(super) fn finish(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
    ) -> CandidateSolutions<'db> {
        if self.pending.is_empty() {
            return CandidateSolutions::Unsatisfiable;
        }

        self.pending.sort_by(|pending1, pending2| {
            let source_orders1 = pending1
                .typevars
                .iter()
                .map(|(_, source_order)| *source_order);
            let source_orders2 = pending2
                .typevars
                .iter()
                .map(|(_, source_order)| *source_order);
            source_orders1.cmp(source_orders2)
        });

        let result = self
            .pending
            .drain(..)
            .map(|pending| pending.into_candidate(db, env, storage))
            .collect();
        CandidateSolutions::Constrained(result)
    }
}

impl PendingCandidateSolution {
    fn into_candidate<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
    ) -> CandidateSolution<'db> {
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>> =
            FxIndexMap::default();

        for (constraint, _) in self.typevars {
            let constraint = storage.constraint_data(constraint);
            match constraint {
                Constraint::ConcreteLower(lower) => {
                    let bounds = mappings.entry(lower.typevar).or_default();
                    bounds.add_lower(lower.provenance, lower.bound);
                }
                Constraint::ConcreteUpper(upper) => {
                    let bounds = mappings.entry(upper.typevar).or_default();
                    bounds.add_upper(upper.provenance, upper.bound);
                }
                Constraint::ConcreteEquivalence(equivalence) => {
                    let bounds = mappings.entry(equivalence.typevar).or_default();
                    bounds.add_lower(equivalence.provenance, equivalence.bound);
                    bounds.add_upper(equivalence.provenance, equivalence.bound);
                }
                Constraint::TypeVarRange(bound) => {
                    let bounds = mappings.entry(bound.left).or_default();
                    bounds.add_upper(bound.provenance, Type::TypeVar(bound.right));
                    let bounds = mappings.entry(bound.right).or_default();
                    bounds.add_lower(bound.provenance, Type::TypeVar(bound.left));
                }
                Constraint::TypeVarEquivalence(bound) => {
                    let (left, right) = bound.in_builder(db, storage);
                    let bounds = mappings.entry(left).or_default();
                    bounds.add_lower(bound.provenance, Type::TypeVar(right));
                    bounds.add_upper(bound.provenance, Type::TypeVar(right));
                    let bounds = mappings.entry(right).or_default();
                    bounds.add_lower(bound.provenance, Type::TypeVar(left));
                    bounds.add_upper(bound.provenance, Type::TypeVar(left));
                }
            }
        }

        let typevars = mappings
            .drain(..)
            .map(|(bound_typevar, bounds)| bounds.finish(db, env, bound_typevar))
            .collect();

        CandidateSolution {
            typevars,
            validity: SolutionValidity::Valid,
        }
    }
}
