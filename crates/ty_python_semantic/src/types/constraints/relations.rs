//! Relations between whole types on a constraint path, independent of their internal structure.

use rustc_hash::FxHashSet;

use super::{
    Constraint, ConstraintBound, ConstraintBoundsBuilder, ConstraintSetBuilder, PathBound,
};
use crate::types::cyclic::PairVisitor;
use crate::types::graph::DependencyGraph;
use crate::types::typevar::TypeVarSet;
use crate::types::{BoundTypeVarInstance, Type};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

/// Quotients the path's subtype relations by mutual reachability. Occurrences inside a type
/// constructor are deliberately not edges: equality does not imply structural recursion.
pub(in crate::types) struct PathRelations<'db> {
    inferable: TypeVarSet<'db>,
    types: FxIndexSet<Type<'db>>,
    components: Vec<usize>,
    supertypes: Vec<FxHashSet<usize>>,
    proofs: PairVisitor<'db, Self, bool>,
}

impl<'db> PathRelations<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &[Constraint<'db>],
        inferable: TypeVarSet<'db>,
    ) -> Self {
        let mut types = FxIndexSet::default();
        let mut edges = Vec::new();
        for constraint in constraints {
            let subject = types.insert_full(Type::TypeVar(constraint.typevar())).0;
            if let Some(lower) = constraint.stored_lower_bound()
                && lower.ty().is_fully_static(db, env)
            {
                let lower = types.insert_full(lower.ty().resolve_type_alias(db)).0;
                edges.push((lower, subject));
            }
            if let Some(upper) = constraint.stored_upper_bound()
                && upper.ty().is_fully_static(db, env)
            {
                let upper = types.insert_full(upper.ty().resolve_type_alias(db)).0;
                edges.push((subject, upper));
            }
        }
        let mut successors = vec![Vec::new(); types.len()];
        for (lower, upper) in edges {
            successors[lower].push(upper);
        }
        let graph = DependencyGraph::new(successors);
        let groups = graph.components(0..types.len());
        let mut components = vec![0; types.len()];
        for (component, members) in groups.iter().enumerate() {
            for &member in members {
                components[member] = component;
            }
        }
        let mut supertypes: Vec<FxHashSet<usize>> = vec![FxHashSet::default(); groups.len()];
        for (component, members) in groups.iter().enumerate() {
            let mut reachable = FxHashSet::default();
            for &member in members {
                for &successor in &graph.dependencies[member] {
                    let successor = components[successor];
                    if successor != component {
                        reachable.insert(successor);
                        reachable.extend(supertypes[successor].iter().copied());
                    }
                }
            }
            supertypes[component] = reachable;
        }
        Self {
            inferable,
            types,
            components,
            supertypes,
            proofs: PairVisitor::new(false),
        }
    }

    fn component(&self, db: &'db dyn Db, ty: Type<'db>) -> Option<usize> {
        self.types
            .get_index_of(&ty.resolve_type_alias(db))
            .map(|index| self.components[index])
    }

    pub(super) fn collect_bounds(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &[Constraint<'db>],
    ) -> Box<[PathBound<'db>]> {
        let builder = ConstraintSetBuilder::new();
        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();
        for constraint in constraints {
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

        mappings
            .into_iter()
            .map(|(typevar, bounds)| {
                let candidate = self.lower_candidate(db, env, typevar, &bounds, &builder);
                let mut bounds = bounds.finish(db, env, typevar);
                bounds.candidate_lower = candidate;
                bounds
            })
            .collect()
    }

    fn lower_candidate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarInstance<'db>,
        bounds: &ConstraintBoundsBuilder<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Option<Type<'db>> {
        let mut result: Vec<ConstraintBound<'db>> = Vec::new();
        let mut has_equal_variables = false;
        for lower in bounds
            .evidence_lower
            .iter()
            .copied()
            .map(ConstraintBound::Evidence)
            .chain(
                bounds
                    .validity_lower
                    .iter()
                    .copied()
                    .map(ConstraintBound::Validity),
            )
        {
            if let Type::TypeVar(other) = lower.ty().resolve_type_alias(db)
                && other.is_inferable(db, self.inferable)
                && self.component(db, Type::TypeVar(other))
                    == self.component(db, Type::TypeVar(typevar))
            {
                has_equal_variables = true;
                continue;
            }
            // Keep evidence separate from validity even when both describe the same class.
            if result.iter().any(|existing| {
                std::mem::discriminant(existing) == std::mem::discriminant(&lower)
                    && self.is_subtype(db, env, lower.ty(), existing.ty(), builder)
            }) {
                continue;
            }
            result.retain(|existing| {
                std::mem::discriminant(existing) != std::mem::discriminant(&lower)
                    || !self.is_subtype(db, env, existing.ty(), lower.ty(), builder)
            });
            result.push(lower);
        }
        let mut candidate = ConstraintBoundsBuilder::default();
        for bound in result {
            candidate.add_lower(db, env, bound);
        }
        let candidate = candidate.finish(db, env, typevar);
        candidate.evidence_lower?;
        let candidate = candidate.effective_lower(db, env);
        // Removing equality edges must preserve the set of solutions. A mere lower bound
        // cannot replace an unknown whose value is still restricted by other variables.
        if has_equal_variables
            && !self.is_subtype(db, env, Type::TypeVar(typevar), candidate, builder)
        {
            return None;
        }
        Some(candidate)
    }

    pub(in crate::types) fn contains_subtype(
        &self,
        db: &'db dyn Db,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> bool {
        match (self.component(db, lower), self.component(db, upper)) {
            (Some(lower), Some(upper)) => lower == upper || self.supertypes[lower].contains(&upper),
            _ => false,
        }
    }

    /// Reusing the obligation being proved is not evidence for a subtype relation.
    /// Only successful proofs are reusable: a failed proof can depend on the active cycle.
    pub(in crate::types) fn prove_subtype(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        prove: impl FnOnce() -> bool,
    ) -> bool {
        self.proofs
            .try_visit(db, (source, target), |proved| *proved, prove)
            .unwrap_or(false)
    }

    pub(in crate::types) fn upper_types(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
    ) -> impl Iterator<Item = Type<'db>> + '_ {
        let component = self.component(db, source);
        self.types
            .iter()
            .copied()
            .enumerate()
            .filter_map(move |(index, ty)| {
                let component = component?;
                let other = self.components[index];
                ((component == other && !ty.is_type_var())
                    || self.supertypes[component].contains(&other))
                .then_some(ty)
            })
    }

    pub(in crate::types) fn lower_types(
        &self,
        db: &'db dyn Db,
        target: Type<'db>,
    ) -> impl Iterator<Item = Type<'db>> + '_ {
        let component = self.component(db, target);
        self.types
            .iter()
            .copied()
            .enumerate()
            .filter_map(move |(index, ty)| {
                let component = component?;
                let other = self.components[index];
                ((component == other && !ty.is_type_var())
                    || self.supertypes[other].contains(&component))
                .then_some(ty)
            })
    }

    fn is_subtype(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> bool {
        if !lower.is_fully_static(db, env) || !upper.is_fully_static(db, env) {
            return false;
        }
        if self.contains_subtype(db, lower, upper) {
            return true;
        }
        lower
            .when_subtype_of_assuming(db, env, upper, self, builder, TypeVarSet::None)
            .is_always_satisfied(db, env)
    }
}
