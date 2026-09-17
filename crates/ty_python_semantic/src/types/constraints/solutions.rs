use std::ops::ControlFlow;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::constraints::relations::{ConstraintRelation, PathRelations};
use crate::types::constraints::variables::ConstraintProvenance;
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, Constraint, ConstraintAssignment, ConstraintId,
    ConstraintSetBuilder, ConstraintSetStorage, NodeId, PathBound, PathBoundBuilder, PathBounds,
    SolutionLimits,
};
use crate::types::signatures::Parameters;
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarSet};
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

pub(super) struct SolutionWalker<'db> {
    source_orders: FxIndexSet<ConstraintId>,
    inferable: TypeVarSet<'db>,
    sorted_paths: Vec<(Vec<usize>, Box<[PathBound<'db>]>)>,
}

impl<'db> SolutionWalker<'db> {
    pub(super) fn new(source_orders: FxIndexSet<ConstraintId>, inferable: TypeVarSet<'db>) -> Self {
        Self {
            source_orders,
            inferable,
            sorted_paths: Vec::default(),
        }
    }

    pub(super) fn visit_node<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        limits: &mut L,
    ) -> ControlFlow<L::Break> {
        self.walk(db, env, storage, node, limits, |walker, path, limits| {
            walker.collect_path(db, env, path, limits)
        })
    }

    pub(super) fn find_path(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        limits: &mut impl SolutionLimits<Break = Option<bool>>,
    ) -> ControlFlow<Option<bool>> {
        self.walk(db, env, storage, node, limits, |_, _, _| {
            ControlFlow::Break(Some(true))
        })
    }

    fn walk<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        limits: &mut L,
        mut visit: impl FnMut(&mut Self, SolutionBounds<'db>, &mut L) -> ControlFlow<L::Break>,
    ) -> ControlFlow<L::Break> {
        let mut pending = vec![(node, None, SolutionBounds::default())];
        while let Some((node, source, mut path)) = pending.pop() {
            limits.visit_node()?;
            if node == ALWAYS_FALSE {
                continue;
            }
            if node == ALWAYS_TRUE {
                if let Some((provenance, source)) = path.comparisons.pop() {
                    let (node, _) = storage.load(
                        db,
                        env,
                        &provenance.lower.when_constraint_set_assignable_to_owned(
                            db,
                            env,
                            provenance.upper,
                        ),
                    );
                    pending.push((node, Some((source, provenance.provenance)), path));
                    continue;
                }
                let constraints: Vec<_> = path.facts.keys().copied().collect();
                let relations = PathRelations::new(db, env, &constraints);
                if path
                    .negative
                    .iter()
                    .any(|constraint| path.implies(db, env, &relations, *constraint))
                {
                    continue;
                }
                let implied = self
                    .source_orders
                    .iter()
                    .enumerate()
                    .find_map(|(source, id)| {
                        (!path.assignments.contains(&id.when_true())
                            && path.constrains(db, storage.constraint_data(*id))
                            && path.implies(db, env, &relations, storage.constraint_data(*id)))
                        .then_some((source, *id))
                    });
                if let Some((source, id)) = implied {
                    if path.assign(
                        db,
                        storage,
                        self.inferable,
                        id.when_true(),
                        (source, ConstraintProvenance::Evidence),
                    ) {
                        pending.push((ALWAYS_TRUE, None, path));
                    }
                    continue;
                }
                visit(self, path, limits)?;
                continue;
            }

            let interior = storage.interior_node_data(node);
            let inherited_source = source;
            let source = source.unwrap_or_else(|| {
                (
                    self.source_orders
                        .get_index_of(&interior.constraint)
                        .unwrap_or(self.source_orders.len()),
                    ConstraintProvenance::Evidence,
                )
            });
            for (assignment, child) in [
                (interior.constraint.when_false(), interior.if_false),
                (
                    interior.constraint.when_unconstrained(),
                    interior.if_uncertain,
                ),
                (interior.constraint.when_true(), interior.if_true),
            ] {
                if child == ALWAYS_FALSE {
                    continue;
                }
                let mut path = path.clone();
                if path.assign(db, storage, self.inferable, assignment, source) {
                    pending.push((child, inherited_source, path));
                }
            }
        }
        ControlFlow::Continue(())
    }

    fn collect_path<L: SolutionLimits>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        path: SolutionBounds<'db>,
        limits: &mut L,
    ) -> ControlFlow<L::Break> {
        let mut facts: Vec<_> = path.facts.into_iter().collect();
        facts.sort_by_key(|(_, source)| *source);
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>> =
            FxIndexMap::default();
        for (relation, _) in &facts {
            if let Type::TypeVar(variable) = relation.upper.resolve_type_alias(db) {
                mappings.entry(variable).or_default().add_lower(
                    db,
                    env,
                    relation.provenance,
                    relation.lower,
                );
            }
            if let Type::TypeVar(variable) = relation.lower.resolve_type_alias(db) {
                mappings.entry(variable).or_default().add_upper(
                    db,
                    env,
                    relation.provenance,
                    relation.upper,
                );
            }
        }

        let bounds: Box<[_]> = mappings
            .into_iter()
            .map(|(typevar, bounds)| bounds.finish(db, env, typevar))
            .collect();
        if !self
            .sorted_paths
            .iter()
            .any(|(_, previous)| *previous == bounds)
        {
            limits.satisfied_path()?;
            self.sorted_paths.push((
                facts.into_iter().map(|(_, source)| source).collect(),
                bounds,
            ));
        }
        ControlFlow::Continue(())
    }

    pub(super) fn finish(mut self) -> PathBounds<'db> {
        if self.sorted_paths.is_empty() {
            return PathBounds::Unsatisfiable;
        }
        self.sorted_paths
            .sort_by(|(left, _), (right, _)| left.cmp(right));
        PathBounds::Constrained(
            self.sorted_paths
                .into_iter()
                .map(|(_, bounds)| bounds)
                .collect(),
        )
    }
}

/// Propagates existing bounds along variable edges without substituting inside constructors.
/// Structural comparisons add constituent bounds; they do not instantiate recursive variables.
/// Recursive bounds remain symbolic for subsequent dependency resolution.
#[derive(Clone, Default)]
struct SolutionBounds<'db> {
    assignments: FxHashSet<ConstraintAssignment>,
    facts: FxIndexMap<ConstraintRelation<'db>, usize>,
    by_variable: FxHashMap<BoundTypeVarIdentity<'db>, Vec<ConstraintRelation<'db>>>,
    negative: Vec<Constraint<'db>>,
    compared: FxHashSet<ConstraintRelation<'db>>,
    comparisons: Vec<(ConstraintRelation<'db>, usize)>,
}

impl<'db> SolutionBounds<'db> {
    fn constrains(&self, db: &'db dyn Db, constraint: Constraint<'db>) -> bool {
        constraint.types().any(|ty| {
            ty.resolve_type_alias(db)
                .as_typevar()
                .is_some_and(|variable| self.by_variable.contains_key(&variable.identity(db)))
        })
    }

    fn implies(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        relations: &PathRelations<'db>,
        constraint: Constraint<'db>,
    ) -> bool {
        let builder = ConstraintSetBuilder::new();
        constraint.relations().all(|relation| {
            if relation.lower.is_never()
                || relation.upper.is_object()
                || relations.is_subtype(db, env, relation.lower, relation.upper, &builder)
            {
                return true;
            }
            // Missing endpoints participate in implication without becoming inference evidence.
            // In particular, `int <= T` implies `T <= Any` through its implicit upper bound.
            if let Type::TypeVar(variable) = relation.upper.resolve_type_alias(db) {
                let bottom = if variable.is_paramspec(db) && variable.paramspec_attr(db).is_none() {
                    Type::paramspec_value_callable(db, Parameters::bottom())
                } else {
                    Type::Never
                };
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
                        relation
                            .lower
                            .when_constraint_set_assignable_to_owned(db, env, lower)
                            .query(|_, when| when.is_trivially_always_satisfied())
                    })
                {
                    return true;
                }
            }
            if let Type::TypeVar(variable) = relation.lower.resolve_type_alias(db) {
                let top = if variable.is_paramspec(db) && variable.paramspec_attr(db).is_none() {
                    Type::paramspec_value_callable(db, Parameters::top())
                } else {
                    Type::object()
                };
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
                        upper
                            .when_constraint_set_assignable_to_owned(db, env, relation.upper)
                            .query(|_, when| when.is_trivially_always_satisfied())
                    })
                {
                    return true;
                }
            }
            false
        })
    }

    fn assign(
        &mut self,
        db: &'db dyn Db,
        storage: &ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        assignment: ConstraintAssignment,
        (source, provenance): (usize, ConstraintProvenance),
    ) -> bool {
        if matches!(assignment, ConstraintAssignment::Unconstrained(_)) {
            return true;
        }
        if self.assignments.contains(&assignment.negated()) {
            return false;
        }
        if !self.assignments.insert(assignment) {
            return true;
        }
        let constraint = storage.constraint_data(assignment.constraint());
        if matches!(assignment, ConstraintAssignment::Negative(_)) {
            self.negative.push(constraint);
            return true;
        }
        let mut pending: Vec<_> = constraint
            .relations()
            .map(|mut relation| {
                if provenance == ConstraintProvenance::Validity {
                    relation.provenance = ConstraintProvenance::Validity;
                }
                (relation, source)
            })
            .collect();
        while let Some((fact, source)) = pending.pop() {
            // Reflexive variable edges are tautologies, not inference evidence.
            if matches!((fact.lower.resolve_type_alias(db), fact.upper.resolve_type_alias(db)), (Type::TypeVar(left), Type::TypeVar(right)) if left.is_same_typevar_as(db, right))
                || self.facts.contains_key(&fact)
            {
                continue;
            }
            self.facts.insert(fact, source);
            // A transparent alias of a typevar is a variable edge, not a constructor bound.
            for variable in [fact.lower, fact.upper]
                .into_iter()
                .filter_map(|ty| ty.resolve_type_alias(db).as_typevar())
            {
                let adjacent = self.by_variable.entry(variable.identity(db)).or_default();
                adjacent.push(fact);
                for &other in adjacent.iter() {
                    let source = source.max(self.facts[&other]);
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
                        let lower_type = lower.lower.resolve_type_alias(db);
                        let upper_type = upper.upper.resolve_type_alias(db);
                        // Fixed variables are not substitution targets. Structural comparisons
                        // still constrain nested variables, as in `list[T] <= N <= list[str]`.
                        if !variable.is_inferable(db, inferable)
                            && (lower_type.is_type_var() || upper_type.is_type_var())
                        {
                            continue;
                        }
                        let relation = ConstraintRelation {
                            lower: lower.lower,
                            upper: upper.upper,
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
