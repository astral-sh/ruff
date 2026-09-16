//! Finite bounds for closed systems of monotone lower-bound constraints.
//!
//! With unbounded variables and static, covariant bounds, the least solution of `L(X) ≤ X`
//! is a fixed point of `L`. An upper bound adds no restriction to that solution if `L(X) ≤ U(X)`
//! or `X ≤ U(X)` holds for every assignment. Keep these systems as variable dependencies instead
//! of enumerating repeated substitutions through recursive bounds.

use std::ops::ControlFlow;

use super::{
    ALWAYS_FALSE, Constraint, ConstraintId, ConstraintProvenance, ConstraintSetStorage, Node,
    NodeId, PathBound, PathBoundBuilder, SolutionLimits, UnboundedSolutionLimits,
};
use crate::types::generics::{ApplySpecialization, GenericContext};
use crate::types::graph::DependencyGraph;
use crate::types::typevar::TypeVarSet;
use crate::types::variance::VarianceInferable;
use crate::types::visitor::any_over_type;
use crate::types::{
    BoundTypeVarInstance, Type, TypeContext, TypeMapping, TypeVarBoundOrConstraints,
    TypeVarVariance, UnionType,
};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

impl NodeId {
    /// Recognizes conjunctions whose static, covariant bounds have a satisfying fixed point.
    pub(super) fn monotone_conjunction_is_satisfiable<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
    ) -> bool {
        let ControlFlow::Continue(Some(constraints)) =
            self.positive_conjunction(storage, &mut UnboundedSolutionLimits)
        else {
            return false;
        };
        let constraints: Vec<_> = constraints
            .into_iter()
            .map(|(_, constraint)| constraint)
            .collect();
        let inferable = TypeVarSet::from_typevars(
            db,
            constraints
                .iter()
                .flat_map(|constraint| constraint.types())
                .filter_map(Type::as_typevar),
        );
        PathBound::from_monotone_constraints(db, env, storage, &constraints, inferable).is_some()
    }

    fn positive_conjunction<'db, L: SolutionLimits>(
        mut self,
        storage: &ConstraintSetStorage<'db>,
        limits: &mut L,
    ) -> ControlFlow<L::Break, Option<Vec<(ConstraintId, Constraint<'db>)>>> {
        let mut constraints = Vec::new();
        loop {
            limits.visit_node()?;
            match self.node() {
                Node::AlwaysTrue => return ControlFlow::Continue(Some(constraints)),
                Node::AlwaysFalse => return ControlFlow::Continue(None),
                Node::Interior(_) => {
                    let interior = storage.interior_node_data(self);
                    if interior.if_false != ALWAYS_FALSE || interior.if_uncertain != ALWAYS_FALSE {
                        return ControlFlow::Continue(None);
                    }
                    constraints.push((
                        interior.constraint,
                        storage.constraint_data(interior.constraint),
                    ));
                    self = interior.if_true;
                }
            }
        }
    }
}

impl<'db> PathBound<'db> {
    /// Collects symbolic lower bounds when propagation cannot further restrict their least
    /// fixed point. Systems with other constraints retain the ordinary sequent-based traversal.
    pub(super) fn collect_monotone_conjunction<L: SolutionLimits>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_orders: &FxIndexSet<ConstraintId>,
        node: NodeId,
        inferable: TypeVarSet<'db>,
        limits: &mut L,
    ) -> ControlFlow<L::Break, Option<Box<[Self]>>> {
        let Some(constraints) = node.positive_conjunction(storage, limits)? else {
            return ControlFlow::Continue(None);
        };
        let mut ordered = Vec::with_capacity(constraints.len());
        for (id, constraint) in constraints {
            // Solution selection here requires evidence for every bound.
            if constraint.provenance() != ConstraintProvenance::Evidence {
                return ControlFlow::Continue(None);
            }
            let Some(order) = source_orders.get_index_of(&id) else {
                return ControlFlow::Continue(None);
            };
            ordered.push((order, constraint));
        }
        let mut constraints = ordered;
        constraints.sort_by_key(|(order, _)| *order);
        let constraints: Vec<_> = constraints.into_iter().map(|(_, c)| c).collect();
        ControlFlow::Continue(Self::from_monotone_constraints(
            db,
            env,
            storage,
            &constraints,
            inferable,
        ))
    }

    fn from_monotone_constraints(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        constraints: &[Constraint<'db>],
        inferable: TypeVarSet<'db>,
    ) -> Option<Box<[PathBound<'db>]>> {
        let mut bounds: FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>> =
            FxIndexMap::default();
        for constraint in constraints {
            // Satisfiability depends on the inequalities, regardless of their provenance.
            // Inference only calls this proof for evidence bounds.
            match *constraint {
                Constraint::ConcreteLower(lower) => {
                    bounds.entry(lower.typevar).or_default().add_lower(
                        db,
                        env,
                        ConstraintProvenance::Evidence,
                        lower.bound,
                    );
                }
                Constraint::ConcreteUpper(upper) => {
                    bounds.entry(upper.typevar).or_default().add_upper(
                        db,
                        env,
                        ConstraintProvenance::Evidence,
                        upper.bound,
                    );
                }
                Constraint::ConcreteEquivalence(equivalence) => {
                    let bounds = bounds.entry(equivalence.typevar).or_default();
                    bounds.add_lower(db, env, ConstraintProvenance::Evidence, equivalence.bound);
                    bounds.add_upper(db, env, ConstraintProvenance::Evidence, equivalence.bound);
                }
                Constraint::TypeVarRange(range) => {
                    bounds.entry(range.left).or_default().add_upper(
                        db,
                        env,
                        ConstraintProvenance::Evidence,
                        Type::TypeVar(range.right),
                    );
                    bounds.entry(range.right).or_default().add_lower(
                        db,
                        env,
                        ConstraintProvenance::Evidence,
                        Type::TypeVar(range.left),
                    );
                }
                Constraint::TypeVarEquivalence(equivalence) => {
                    // Preserve the same variable order as ordinary solution collection.
                    let (left, right) = equivalence.in_builder(db, storage);
                    for (variable, other) in [(left, right), (right, left)] {
                        let bounds = bounds.entry(variable).or_default();
                        bounds.add_lower(
                            db,
                            env,
                            ConstraintProvenance::Evidence,
                            Type::TypeVar(other),
                        );
                        bounds.add_upper(
                            db,
                            env,
                            ConstraintProvenance::Evidence,
                            Type::TypeVar(other),
                        );
                    }
                }
            }
        }
        let variables: FxIndexSet<_> = bounds.keys().copied().collect();
        if variables.is_empty()
            || variables.iter().any(|variable| {
                !variable.is_inferable(db, inferable)
                    || !matches!(variable.require_bound_or_constraints(db, env),
                        TypeVarBoundOrConstraints::UpperBound(bound)
                            if bound.resolve_type_alias(db).is_object())
            })
        {
            return None;
        }
        for bounds in bounds.values() {
            for bound in bounds
                .evidence_lower
                .iter()
                .copied()
                .chain(bounds.upper.iter_evidence())
            {
                if !bound.is_fully_static(db, env)
                    || bound.has_provisional_marker(db, env)
                    || any_over_type(db, env, bound, true, |ty| {
                        ty.as_typevar()
                            .is_some_and(|variable| !variables.contains(&variable))
                    })
                    || variables.iter().any(|variable| {
                        !bound
                            .variance_of(db, env, variable.identity(db))
                            .evaluate(db)
                            .is_covariant()
                    })
                {
                    return None;
                }
            }
        }

        let lower: Vec<_> = bounds
            .values()
            .map(|bounds| &bounds.evidence_lower)
            .collect();
        // Mutual bare-variable bounds prove equality, independently of constructor recursion.
        let graph = DependencyGraph::new(
            lower
                .iter()
                .map(|bounds| {
                    bounds
                        .iter()
                        .filter_map(|bound| {
                            bound.as_typevar().and_then(|v| variables.get_index_of(&v))
                        })
                        .collect()
                })
                .collect(),
        );
        let groups = graph.components(0..variables.len());
        let mut representatives = vec![0; variables.len()];
        for group in &groups {
            let representative = *group.iter().min()?;
            for &variable in group {
                representatives[variable] = representative;
            }
        }
        let context = GenericContext::from_typevar_instances(db, env, variables.iter().copied());
        let replacements: Vec<_> = representatives
            .iter()
            .map(|&index| Type::TypeVar(variables[index]))
            .collect();
        let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: &replacements,
            skip: None,
        });
        let mut candidates = FxIndexMap::default();
        for group in &groups {
            let representative = representatives[group[0]];
            let mut elements = Vec::new();
            for &index in group {
                for &bound in lower[index] {
                    let mapped =
                        bound.apply_type_mapping(db, env, &mapping, TypeContext::default());
                    if mapped != Type::TypeVar(variables[representative]) {
                        elements.push(mapped);
                    }
                }
            }
            // An alias cycle without a defining bound does not supply a type to infer.
            if elements.is_empty() {
                return None;
            }
            let candidate = UnionType::from_elements(db, env, elements);
            candidates.insert(representative, candidate);
        }
        let mut selected: Vec<_> = representatives
            .iter()
            .map(|representative| candidates[representative])
            .collect();
        let dependencies = DependencyGraph::new(
            selected
                .iter()
                .map(|candidate| {
                    variables
                        .iter()
                        .enumerate()
                        .filter_map(|(index, variable)| {
                            (candidate
                                .variance_of(db, env, variable.identity(db))
                                .evaluate(db)
                                != TypeVarVariance::Bivariant)
                                .then_some(index)
                        })
                        .collect()
                })
                .collect(),
        );
        // Substitute acyclic dependencies once; keep cyclic references as finite equations.
        let mut resolved: Vec<_> = variables.iter().copied().map(Type::TypeVar).collect();
        for component in dependencies.components(0..variables.len()) {
            for &index in &component {
                selected[index] = selected[index].apply_type_mapping(
                    db,
                    env,
                    &TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                        generic_context: context,
                        types: &resolved,
                        skip: None,
                    }),
                    TypeContext::default(),
                );
            }
            if component.len() == 1
                && !dependencies.dependencies[component[0]].contains(&component[0])
            {
                let index = component[0];
                resolved[index] = selected[index];
            }
        }
        // Substitute known definitions on both sides before checking additional upper bounds.
        let resolved_mapping = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: &resolved,
            skip: None,
        });
        for (index, variable) in variables.iter().enumerate() {
            for bound in bounds[variable].upper.iter_evidence() {
                if bound.is_type_var() {
                    continue;
                }
                let mapped = bound
                    .apply_type_mapping(db, env, &mapping, TypeContext::default())
                    .apply_type_mapping(db, env, &resolved_mapping, TypeContext::default());
                if !selected[index].is_constraint_set_subtype_of(db, env, mapped)
                    && !Type::TypeVar(variables[representatives[index]])
                        .is_constraint_set_subtype_of(db, env, mapped)
                {
                    return None;
                }
            }
        }
        Some(
            bounds
                .into_iter()
                .zip(selected)
                .map(|((variable, mut bounds), selected)| {
                    bounds.evidence_lower.clear();
                    bounds.add_lower(db, env, ConstraintProvenance::Evidence, selected);
                    bounds.finish(db, env, variable)
                })
                .collect(),
        )
    }
}
