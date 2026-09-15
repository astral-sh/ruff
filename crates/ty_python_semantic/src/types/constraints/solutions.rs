use std::ops::ControlFlow;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::constraints::relations::PathRelations;
use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, Constraint, ConstraintAssignment, ConstraintBound,
    ConstraintBoundsBuilder, ConstraintId, ConstraintSetBuilder, ConstraintSetStorage, NodeId,
    PathBound, PathBounds, SolutionLimits,
};
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
                        &provenance
                            .lower_bound(db)
                            .ty()
                            .when_constraint_set_assignable_to_owned(
                                db,
                                env,
                                provenance.upper_bound(db).ty(),
                            ),
                    );
                    pending.push((node, Some((source, provenance)), path));
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
                            && path
                                .by_variable
                                .contains_key(&storage.constraint_data(*id).typevar().identity(db))
                            && path.implies(db, env, &relations, storage.constraint_data(*id)))
                        .then_some((source, *id))
                    });
                if let Some((source, id)) = implied {
                    if path.assign(
                        db,
                        storage,
                        self.inferable,
                        id.when_true(),
                        (source, storage.constraint_data(id)),
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
                    storage.constraint_data(interior.constraint),
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
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();
        for (constraint, _) in &facts {
            let typevar = constraint.typevar();
            let bounds = mappings.entry(typevar).or_default();
            if let Some(lower) = constraint.stored_lower_bound() {
                bounds.add_lower(db, env, lower);
            }
            if let Some(upper) = constraint.stored_upper_bound() {
                bounds.add_upper(db, env, upper);
            }
            if let Some(lower) = constraint.stored_lower_bound()
                && let Type::TypeVar(other) = lower.ty().resolve_type_alias(db)
            {
                mappings.entry(other).or_default().add_upper(
                    db,
                    env,
                    lower.with_type(Type::TypeVar(typevar)),
                );
            }
            if let Some(upper) = constraint.stored_upper_bound()
                && let Type::TypeVar(other) = upper.ty().resolve_type_alias(db)
            {
                mappings.entry(other).or_default().add_lower(
                    db,
                    env,
                    upper.with_type(Type::TypeVar(typevar)),
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
    facts: FxIndexMap<Constraint<'db>, usize>,
    by_variable: FxHashMap<BoundTypeVarIdentity<'db>, Vec<Constraint<'db>>>,
    negative: Vec<Constraint<'db>>,
    compared: FxHashSet<(ConstraintBound<'db>, ConstraintBound<'db>)>,
    comparisons: Vec<(Constraint<'db>, usize)>,
}

impl<'db> SolutionBounds<'db> {
    fn implies(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        relations: &PathRelations<'db>,
        constraint: Constraint<'db>,
    ) -> bool {
        let builder = ConstraintSetBuilder::new();
        let subject = Type::TypeVar(constraint.typevar());
        let lower = constraint.lower_bound(db).ty();
        let upper = constraint.upper_bound(db).ty();
        // Missing endpoints still participate in implication: e.g. `int <= T` implies `T <= Any`
        // through its implicit upper bound, without adding that bound as inference evidence.
        let lower_holds = lower.is_never()
            || relations.is_subtype(db, env, lower, subject, &builder)
            || self
                .by_variable
                .get(&constraint.typevar().identity(db))
                .into_iter()
                .flatten()
                .any(|fact| {
                    lower
                        .when_constraint_set_assignable_to_owned(db, env, fact.lower_bound(db).ty())
                        .query(|_, when| when.is_trivially_always_satisfied())
                });
        let upper_holds = upper.is_object()
            || relations.is_subtype(db, env, subject, upper, &builder)
            || self
                .by_variable
                .get(&constraint.typevar().identity(db))
                .into_iter()
                .flatten()
                .any(|fact| {
                    fact.upper_bound(db)
                        .ty()
                        .when_constraint_set_assignable_to_owned(db, env, upper)
                        .query(|_, when| when.is_trivially_always_satisfied())
                });
        lower_holds && upper_holds
    }

    fn assign(
        &mut self,
        db: &'db dyn Db,
        storage: &ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        assignment: ConstraintAssignment,
        (source, provenance): (usize, Constraint<'db>),
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
        let mut pending = Vec::new();
        if let Some(lower) = constraint.stored_lower_bound() {
            pending.push((
                Constraint::new(
                    constraint.typevar(),
                    Some(lower.with_source_provenance(provenance)),
                    None,
                ),
                source,
            ));
        }
        if let Some(upper) = constraint.stored_upper_bound() {
            pending.push((
                Constraint::new(
                    constraint.typevar(),
                    None,
                    Some(upper.with_source_provenance(provenance)),
                ),
                source,
            ));
        }
        while let Some((fact, source)) = pending.pop() {
            // Reflexive variable edges are tautologies, not inference evidence.
            if fact.iter_stored_bounds().any(|bound| matches!(bound.ty(), Type::TypeVar(other) if fact.typevar().is_same_typevar_as(db, other))) || self.facts.contains_key(&fact) {
                continue;
            }
            self.facts.insert(fact, source);
            let variable = fact.typevar();
            self.by_variable
                .entry(variable.identity(db))
                .or_default()
                .push(fact);
            if let Some(lower) = fact.stored_lower_bound()
                && let Type::TypeVar(other) = lower.ty().resolve_type_alias(db)
            {
                pending.push((
                    Constraint::new(other, None, Some(lower.with_type(Type::TypeVar(variable)))),
                    source,
                ));
            }
            if let Some(upper) = fact.stored_upper_bound()
                && let Type::TypeVar(other) = upper.ty().resolve_type_alias(db)
            {
                pending.push((
                    Constraint::new(other, Some(upper.with_type(Type::TypeVar(variable))), None),
                    source,
                ));
            }
            for &other in &self.by_variable[&variable.identity(db)] {
                let other_source = self.facts[&other];
                for (lower, upper) in [
                    (fact.stored_lower_bound(), other.stored_upper_bound()),
                    (other.stored_lower_bound(), fact.stored_upper_bound()),
                ] {
                    let (Some(lower), Some(upper)) = (lower, upper) else {
                        continue;
                    };
                    let lower_type = lower.ty().resolve_type_alias(db);
                    let upper_type = upper.ty().resolve_type_alias(db);
                    // Fixed variables are not substitution targets. Structural comparisons still
                    // constrain their nested variables, as in `list[T] <= N <= list[str]`.
                    if !variable.is_inferable(db, inferable)
                        && (lower_type.is_type_var() || upper_type.is_type_var())
                    {
                        continue;
                    }
                    let source = source.max(other_source);
                    match (lower_type, upper_type) {
                        (Type::TypeVar(lower_variable), _) => {
                            pending.push((
                                Constraint::new(
                                    lower_variable,
                                    None,
                                    Some(ConstraintBound::from_transitive_derivation(
                                        upper.ty(),
                                        lower,
                                        upper,
                                    )),
                                ),
                                source,
                            ));
                        }
                        (_, Type::TypeVar(upper_variable)) => {
                            pending.push((
                                Constraint::new(
                                    upper_variable,
                                    Some(ConstraintBound::from_transitive_derivation(
                                        lower.ty(),
                                        lower,
                                        upper,
                                    )),
                                    None,
                                ),
                                source,
                            ));
                        }
                        _ if lower.ty() != upper.ty()
                            && !lower.ty().is_never()
                            && !upper.ty().is_object()
                            && self.compared.insert((lower, upper)) =>
                        {
                            self.comparisons.push((
                                Constraint::new(variable, Some(lower), Some(upper)),
                                source,
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        true
    }
}
