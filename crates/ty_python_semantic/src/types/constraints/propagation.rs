//! Bounds propagated along variable edges and decomposed by structural comparison.

use std::vec::IntoIter;

use rustc_hash::{FxHashMap, FxHashSet};

use super::TypeVarSolution;
use super::relations::{ConstraintRelation, PathRelations, RelationKind};
use super::resolution::{SolutionType, resolve_solution};
use super::variables::{Constraint, ConstraintProvenance};
use super::{ConstraintAssignment, ConstraintId, ConstraintSetBuilder, ConstraintSetStorage};
use crate::types::Type;
use crate::types::generics::{ApplySpecialization, GenericContext};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarSet};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{BoundTypeVarInstance, TypeContext, TypeMapping};
use crate::{Db, FxIndexMap, ProgramEnvironment};

/// Propagates existing bounds along variable edges without substituting inside constructors.
/// Structural comparisons add constituent bounds; they do not instantiate recursive variables.
/// Recursive bounds remain symbolic for subsequent dependency resolution.
#[derive(Clone, Default)]
pub(super) struct SolutionPath<'db> {
    pub(super) assignments: FxIndexMap<ConstraintAssignment, ConstraintId>,
    pub(super) facts: FxIndexMap<ConstraintRelation<'db>, (usize, ConstraintId)>,
    by_variable: FxHashMap<BoundTypeVarIdentity<'db>, Vec<ConstraintRelation<'db>>>,
    pub(super) negative: Vec<Constraint<'db>>,
    compared: FxHashSet<ConstraintRelation<'db>>,
    pub(super) comparisons: Vec<(ConstraintRelation<'db>, (usize, ConstraintId))>,
    pub(super) inherited_source: Option<(usize, ConstraintId)>,
}

impl<'db> SolutionPath<'db> {
    /// Original and derived bounds, preserving symbolic inference evidence for validation.
    pub(super) fn positive_constraints(
        &self,
    ) -> impl Iterator<Item = (ConstraintId, ConstraintId)> + '_ {
        self.assignments
            .iter()
            .filter_map(|(assignment, source)| match assignment {
                ConstraintAssignment::Positive(id) => Some((*id, *source)),
                _ => None,
            })
    }

    /// Substitute exact bindings when selecting a solution, preserving original inference evidence.
    pub(super) fn resolved_constraints(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
    ) -> Option<IntoIter<(ConstraintId, ConstraintId)>> {
        let bindings = ExactBindings::new(db, env, self.facts.keys().copied());
        let mut constraints = Vec::new();
        for (assignment, source) in &self.assignments {
            let ConstraintAssignment::Positive(id) = assignment else {
                continue;
            };
            let constraint = storage.constraint_data(*id);
            let mut changed = false;
            for ty in constraint.types() {
                let Some(variable) = ty.as_typevar() else {
                    continue;
                };
                for (bound, lower) in [
                    (constraint.lower_bound_for(db, variable), true),
                    (constraint.upper_bound_for(db, variable), false),
                ] {
                    let Some(bound) = bound else { continue };
                    let (mapped, provenance) =
                        bindings.apply(db, env, bound, constraint.provenance());
                    if mapped == bound {
                        continue;
                    }
                    changed = true;
                    let replacements: Vec<_> = if matches!(
                        constraint,
                        Constraint::ConcreteEquivalence(_) | Constraint::TypeVarEquivalence(_)
                    ) {
                        Constraint::new_equivalence_bound(db, env, provenance, variable, mapped)
                            .collect()
                    } else if lower {
                        Constraint::new_lower_bound(db, provenance, variable, mapped).collect()
                    } else {
                        Constraint::new_upper_bound(db, env, provenance, variable, mapped).collect()
                    };
                    for replacement in replacements {
                        constraints.push((
                            storage.intern_constraint(db, env, replacement.ok()?),
                            *source,
                        ));
                    }
                }
            }
            if !changed {
                constraints.push((*id, *source));
            }
        }
        Some(constraints.into_iter())
    }

    /// Whether the path already contains bounds for an endpoint of this constraint.
    pub(super) fn constrains(&self, db: &'db dyn Db, constraint: Constraint<'db>) -> bool {
        constraint.types().any(|ty| {
            ty.resolve_type_alias(db)
                .as_typevar()
                .is_some_and(|variable| self.by_variable.contains_key(&variable.identity(db)))
        })
    }

    /// Prove a constraint from path facts, including implicit range endpoints.
    pub(super) fn implies(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        relations: &PathRelations<'db>,
        constraint: Constraint<'db>,
    ) -> bool {
        let builder = ConstraintSetBuilder::new();
        constraint.relations().all(|relation| {
            if relation.kind == RelationKind::Equivalent {
                return relations.is_equivalent(db, env, relation.lower, relation.upper);
            }
            if relation.lower.is_never()
                || relation.upper.is_object()
                || relations.is_subtype(db, env, relation.lower, relation.upper, &builder)
            {
                return true;
            }
            // Missing endpoints participate in implication without becoming inference evidence.
            // In particular, `int <= T` implies `T <= Any` through its implicit upper bound.
            if let Type::TypeVar(variable) = relation.upper.resolve_type_alias(db) {
                let bottom = variable.domain(db).bottom(db);
                if relation.lower == bottom {
                    return true;
                }
                if self
                    .by_variable
                    .get(&variable.identity(db))
                    .into_iter()
                    .flatten()
                    .any(|fact| {
                        let lower = if fact
                            .upper
                            .resolve_type_alias(db)
                            .as_typevar()
                            .is_some_and(|other| variable.is_same_typevar_as(db, other))
                        {
                            fact.lower
                        } else {
                            bottom
                        };
                        if fact.kind == RelationKind::Equivalent
                            && lower != bottom
                            && relations.is_assignable(db, env, relation.lower, lower)
                        {
                            return true;
                        }
                        relation
                            .lower
                            .bottom_materialization(db, env)
                            .is_constraint_set_subtype_of(
                                db,
                                env,
                                lower.bottom_materialization(db, env),
                            )
                    })
                {
                    return true;
                }
            }
            if let Type::TypeVar(variable) = relation.lower.resolve_type_alias(db) {
                let top = variable.domain(db).top(db);
                if relation.upper == top {
                    return true;
                }
                if self
                    .by_variable
                    .get(&variable.identity(db))
                    .into_iter()
                    .flatten()
                    .any(|fact| {
                        let upper = if fact
                            .lower
                            .resolve_type_alias(db)
                            .as_typevar()
                            .is_some_and(|other| variable.is_same_typevar_as(db, other))
                        {
                            fact.upper
                        } else {
                            top
                        };
                        if fact.kind == RelationKind::Equivalent
                            && upper != top
                            && relations.is_assignable(db, env, upper, relation.upper)
                        {
                            return true;
                        }
                        upper
                            .top_materialization(db, env)
                            .is_constraint_set_subtype_of(
                                db,
                                env,
                                relation.upper.top_materialization(db, env),
                            )
                    })
                {
                    return true;
                }
            }
            false
        })
    }

    /// Add an assignment and propagate variable bounds, returning false on a contradiction.
    pub(super) fn assign(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        assignment: ConstraintAssignment,
        source: (usize, ConstraintId),
    ) -> bool {
        if matches!(assignment, ConstraintAssignment::Unconstrained(_)) {
            return true;
        }
        if self.assignments.contains_key(&assignment.negated()) {
            return false;
        }
        if self.assignments.contains_key(&assignment) {
            return true;
        }
        self.assignments.insert(assignment, source.1);
        let constraint = storage.constraint_data(assignment.constraint());
        if matches!(assignment, ConstraintAssignment::Negative(_)) {
            self.negative.push(constraint);
            return true;
        }
        let mut pending: Vec<_> = constraint
            .relations()
            .map(|relation| (relation, source))
            .collect();
        while let Some((fact, source)) = pending.pop() {
            // Reflexive variable edges are tautologies, not inference evidence.
            if matches!((fact.lower.resolve_type_alias(db), fact.upper.resolve_type_alias(db)), (Type::TypeVar(left), Type::TypeVar(right)) if left.is_same_typevar_as(db, right))
                || self.facts.contains_key(&fact)
            {
                continue;
            }
            self.facts.insert(fact, source);
            // Retain derived variable bounds for solution selection and path cache keys.
            let (variable, bound) =
                if let Some(variable) = fact.upper.resolve_type_alias(db).as_typevar() {
                    (variable, fact.lower)
                } else if let Some(variable) = fact.lower.resolve_type_alias(db).as_typevar() {
                    (variable, fact.upper)
                } else {
                    continue;
                };
            let constraints: Vec<_> = if fact.kind == RelationKind::Equivalent {
                Constraint::new_equivalence_bound(db, env, fact.provenance, variable, bound)
                    .collect()
            } else if fact.upper.resolve_type_alias(db).is_type_var() {
                Constraint::new_lower_bound(db, fact.provenance, variable, bound).collect()
            } else {
                Constraint::new_upper_bound(db, env, fact.provenance, variable, bound).collect()
            };
            for constraint in constraints {
                let Ok(constraint) = constraint else {
                    return false;
                };
                let id = storage.intern_constraint(db, env, constraint);
                if self.assignments.contains_key(&id.when_false()) {
                    return false;
                }
                self.assignments.entry(id.when_true()).or_insert(source.1);
            }
            // A transparent alias of a typevar is a variable edge, not a constructor bound.
            for variable in [fact.lower, fact.upper]
                .into_iter()
                .filter_map(|ty| ty.resolve_type_alias(db).as_typevar())
            {
                let adjacent = self.by_variable.entry(variable.identity(db)).or_default();
                adjacent.push(fact);
                for &other in adjacent.iter() {
                    let other_source = self.facts[&other];
                    let source = if source.0 >= other_source.0 {
                        source
                    } else {
                        other_source
                    };
                    for (lower, upper) in [(fact, other), (other, fact)] {
                        if !lower
                            .upper
                            .resolve_type_alias(db)
                            .as_typevar()
                            .is_some_and(|ty| ty.is_same_typevar_as(db, variable))
                            || !upper
                                .lower
                                .resolve_type_alias(db)
                                .as_typevar()
                                .is_some_and(|ty| ty.is_same_typevar_as(db, variable))
                        {
                            continue;
                        }
                        if lower.lower.is_static_sequent_eligible(db, env)
                            && upper.upper.is_static_sequent_eligible(db, env)
                            && (lower.lower == upper.upper
                                || (variable.is_paramspec(db)
                                    && lower.lower.is_constraint_set_equivalent_to(
                                        db,
                                        env,
                                        upper.upper,
                                    )))
                        {
                            let exact = ConstraintRelation {
                                kind: RelationKind::Equivalent,
                                lower: lower.lower,
                                upper: Type::TypeVar(variable),
                                provenance: ConstraintProvenance::derived(
                                    lower.provenance,
                                    upper.provenance,
                                ),
                            };
                            pending.push((exact, source));
                            pending.push((
                                ConstraintRelation {
                                    lower: exact.upper,
                                    upper: exact.lower,
                                    ..exact
                                },
                                source,
                            ));
                        }
                        let lower_type = lower.lower.resolve_type_alias(db);
                        let upper_type = upper.upper.resolve_type_alias(db);
                        // Declared ranges restrict materializations; their gradual components
                        // are not inference evidence. Exact declared choices remain unchanged.
                        let relation = ConstraintRelation {
                            kind: lower.kind.compose(upper.kind),
                            lower: if lower.provenance == ConstraintProvenance::Validity
                                && lower.kind == RelationKind::Assignable
                            {
                                lower.lower.bottom_materialization(db, env)
                            } else {
                                lower.lower
                            },
                            upper: if upper.provenance == ConstraintProvenance::Validity
                                && upper.kind == RelationKind::Assignable
                            {
                                upper.upper.top_materialization(db, env)
                            } else {
                                upper.upper
                            },
                            provenance: ConstraintProvenance::derived(
                                lower.provenance,
                                upper.provenance,
                            ),
                        };
                        match (lower_type, upper_type) {
                            (Type::TypeVar(variable), _) => {
                                pending.push((
                                    ConstraintRelation {
                                        lower: Type::TypeVar(variable),
                                        ..relation
                                    },
                                    source,
                                ));
                            }
                            (_, Type::TypeVar(variable)) => {
                                pending.push((
                                    ConstraintRelation {
                                        upper: Type::TypeVar(variable),
                                        ..relation
                                    },
                                    source,
                                ));
                            }
                            _ if relation.lower != relation.upper
                                && !relation.lower.is_never()
                                && !relation.upper.is_object()
                                // Gradual bounds are checked when selecting this variable's
                                // solution, but cannot prove a contradiction about a rigid one.
                                && (variable.is_inferable(db, inferable)
                                    || (relation.lower.is_static_sequent_eligible(db, env)
                                        && relation.upper.is_static_sequent_eligible(db, env)))
                                && self.compared.insert(relation) =>
                            {
                                self.comparisons.push((relation, source));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        true
    }
}

/// Acyclic exact bindings can simplify selected bounds without growing the propagation graph.
pub(super) struct ExactBindings<'db> {
    bindings: Vec<(BoundTypeVarInstance<'db>, Type<'db>, ConstraintProvenance)>,
}

impl<'db> ExactBindings<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        facts: impl Iterator<Item = ConstraintRelation<'db>>,
    ) -> Self {
        let mut exact = FxIndexMap::default();
        for fact in facts {
            if fact.kind != RelationKind::Equivalent {
                continue;
            }
            let Some(variable) = fact.upper.as_typevar() else {
                continue;
            };
            // Bare equalities remain symbolic until propagation finds a concrete representative.
            if fact.lower.is_type_var() {
                continue;
            }
            let entry = exact
                .entry(variable)
                .or_insert(Some((fact.lower, fact.provenance)));
            if let Some((ty, provenance)) = entry {
                if *ty == fact.lower {
                    *provenance = ConstraintProvenance::derived(*provenance, fact.provenance);
                } else {
                    *entry = None;
                }
            }
        }
        let mut exact: FxIndexMap<_, _> = exact
            .into_iter()
            .filter_map(|(variable, bound)| bound.map(|bound| (variable, bound)))
            .collect();
        let solution: Vec<_> = exact
            .iter()
            .map(|(variable, (ty, _))| TypeVarSolution {
                bound_typevar: *variable,
                solution: *ty,
            })
            .collect();
        let inferable = TypeVarSet::from_typevars(db, exact.keys().copied());
        let resolved = resolve_solution(db, env, inferable, &solution);
        // A substituted declaration remains validity or mixed evidence, including through chains.
        loop {
            let mut changed = false;
            let previous = exact.clone();
            for (ty, provenance) in exact.values_mut() {
                for (variable, (_, dependency)) in &previous {
                    if any_over_type_expanding_aliases(db, env, *ty, |nested| {
                        nested
                            .as_typevar()
                            .is_some_and(|nested| nested.is_same_typevar_as(db, *variable))
                    }) {
                        let combined = ConstraintProvenance::derived(*provenance, *dependency);
                        changed |= combined != *provenance;
                        *provenance = combined;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Self {
            bindings: exact
                .into_iter()
                .zip(resolved)
                .filter_map(|((variable, (_, provenance)), resolved)| match resolved {
                    SolutionType::Resolved(ty) => Some((variable, ty, provenance)),
                    SolutionType::Unresolved(_) => None,
                })
                .collect(),
        }
    }

    /// Substitute resolved bindings and retain the provenance of every dependency used.
    pub(super) fn apply(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        mut provenance: ConstraintProvenance,
    ) -> (Type<'db>, ConstraintProvenance) {
        let mut variables = Vec::new();
        let mut types = Vec::new();
        for &(variable, replacement, dependency) in &self.bindings {
            if any_over_type_expanding_aliases(db, env, ty, |nested| {
                nested
                    .as_typevar()
                    .is_some_and(|nested| nested.is_same_typevar_as(db, variable))
            }) {
                variables.push(variable);
                types.push(replacement);
                provenance = ConstraintProvenance::derived(provenance, dependency);
            }
        }
        if variables.is_empty() {
            return (ty, provenance);
        }
        let generic_context = GenericContext::from_typevar_instances(db, env, variables);
        (
            ty.apply_type_mapping(
                db,
                env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                    generic_context,
                    types: &types,
                    skip: None,
                }),
                TypeContext::default(),
            ),
            provenance,
        )
    }
}
