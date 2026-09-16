//! Relations between whole types on a constraint path, independent of their internal structure.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};

use super::{ConstraintProvenance, ConstraintSetBuilder, PathBound, PathBoundBuilder};
use crate::types::cyclic::PairVisitor;
use crate::types::graph::DependencyGraph;
use crate::types::typevar::TypeVarSet;
use crate::types::visitor::any_over_type;
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
    proof_revision: Cell<usize>,
    proof_results: RefCell<FxHashMap<(Type<'db>, Type<'db>), (bool, usize)>>,
}

impl<'db> PathRelations<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bounds: &FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>>,
        inferable: TypeVarSet<'db>,
    ) -> Self {
        let mut types = FxIndexSet::default();
        let mut edges = Vec::new();
        for (variable, bounds) in bounds {
            let subject = types.insert_full(Type::TypeVar(*variable)).0;
            for lower in bounds.evidence_lower.iter().chain(&bounds.validity_lower) {
                if lower.is_fully_static(db, env) {
                    let lower = types.insert_full(lower.resolve_type_alias(db)).0;
                    edges.push((lower, subject));
                }
            }
            for upper in bounds.upper.iter_clauses() {
                if upper.is_fully_static(db, env) {
                    let upper = types.insert_full(upper.resolve_type_alias(db)).0;
                    edges.push((subject, upper));
                }
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
            proof_revision: Cell::new(0),
            proof_results: RefCell::default(),
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
        mappings: FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>>,
    ) -> Box<[PathBound<'db>]> {
        let builder = ConstraintSetBuilder::new();
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
        bounds: &PathBoundBuilder<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Option<Type<'db>> {
        let mut result: Vec<(ConstraintProvenance, Type<'db>, bool)> = Vec::new();
        let mut has_equal_variables = false;
        for lower in bounds
            .evidence_lower
            .iter()
            .copied()
            .map(|ty| (ConstraintProvenance::Evidence, ty))
            .chain(
                bounds
                    .validity_lower
                    .iter()
                    .copied()
                    .map(|ty| (ConstraintProvenance::Validity, ty)),
            )
        {
            let dependent = any_over_type(db, env, lower.1, false, |ty| {
                ty.as_typevar()
                    .is_some_and(|typevar| typevar.is_inferable(db, self.inferable))
            });
            if let Type::TypeVar(other) = lower.1.resolve_type_alias(db)
                && other.is_inferable(db, self.inferable)
                && self.component(db, Type::TypeVar(other))
                    == self.component(db, Type::TypeVar(typevar))
            {
                has_equal_variables = true;
                continue;
            }
            // Keep provenance separate, and never discard independent evidence in favor of a
            // pending variable: projecting that variable later could also lose the evidence.
            if result.iter().any(|existing| {
                existing.0 == lower.0
                    && (dependent || !existing.2)
                    && self.is_subtype(db, env, lower.1, existing.1, builder)
            }) {
                continue;
            }
            result.retain(|existing| {
                existing.0 != lower.0
                    || (dependent && !existing.2)
                    || !self.is_subtype(db, env, existing.1, lower.1, builder)
            });
            result.push((lower.0, lower.1, dependent));
        }
        let mut candidate = PathBoundBuilder::default();
        for bound in result {
            candidate.add_lower(db, env, bound.0, bound.1);
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

    /// Reusing an active obligation is not evidence for subtyping. A failed search remains
    /// reusable until another obligation is proved, which can discharge its cyclic dependencies.
    pub(in crate::types) fn prove_subtype(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        prove: impl FnOnce() -> bool,
    ) -> bool {
        let pair = (source, target);
        if let Some(&(proved, revision)) = self.proof_results.borrow().get(&pair)
            && (proved || revision == self.proof_revision.get())
        {
            return proved;
        }
        self.proofs
            .try_visit(
                db,
                pair,
                |proved| *proved,
                || {
                    let revision = self.proof_revision.get();
                    let proved = prove();
                    if proved {
                        self.proof_revision.set(self.proof_revision.get() + 1);
                    }
                    // If this search learned another fact, its earlier failures can be stale.
                    // Keeping the starting revision forces them to be reconsidered.
                    self.proof_results
                        .borrow_mut()
                        .insert(pair, (proved, revision));
                    proved
                },
            )
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
