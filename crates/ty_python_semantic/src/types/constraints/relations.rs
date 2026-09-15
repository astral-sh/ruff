//! Relations between whole types on a constraint path, independent of their internal structure.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};

use super::{Constraint, ConstraintSetBuilder};
use crate::types::Type;
use crate::types::cyclic::PairVisitor;
use crate::types::graph::DependencyGraph;
use crate::types::typevar::TypeVarSet;
use crate::{Db, FxIndexSet, ProgramEnvironment};

/// Quotients the path's subtype relations by mutual reachability. Occurrences inside a type
/// constructor are deliberately not edges: equality does not imply structural recursion.
pub(in crate::types) struct PathRelations<'db> {
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
        constraints: &[Constraint<'db>],
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

    pub(super) fn is_subtype(
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
