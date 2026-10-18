//! Relations between whole types on a constraint path, independent of their internal structure.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};

use super::propagation::ExactBindings;
use super::{Constraint, ConstraintSetBuilder, ConstraintSetStorage, Node, NodeId};
use crate::types::Type;
use crate::types::constraints::variables::ConstraintProvenance;
use crate::types::cyclic::PairVisitor;
use crate::types::graph::DependencyGraph;
use crate::{Db, FxIndexSet, ProgramEnvironment};

/// One directed bound, including the evidence that supports it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct ConstraintRelation<'db> {
    pub(super) kind: RelationKind,
    pub(super) lower: Type<'db>,
    pub(super) upper: Type<'db>,
    pub(super) provenance: ConstraintProvenance,
}

impl ConstraintRelation<'_> {
    /// Declared choices restrict materializations of gradual evidence, not its exact type.
    pub(super) const fn compose(self, other: Self) -> RelationKind {
        match (self.kind, other.kind, self.provenance, other.provenance) {
            (
                RelationKind::Equivalent,
                RelationKind::Equivalent,
                ConstraintProvenance::Evidence,
                ConstraintProvenance::Evidence,
            ) => RelationKind::Equivalent,
            _ => RelationKind::Assignable,
        }
    }
}

/// Exact equality retains gradual types; assignability only restricts their materializations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum RelationKind {
    Assignable,
    Equivalent,
}

impl<'db> Constraint<'db> {
    /// Expands equivalences into both directions without substituting inside either type.
    pub(super) fn relations(self) -> impl Iterator<Item = ConstraintRelation<'db>> {
        let (lower, upper, provenance, equivalent) = match self {
            Self::ConcreteLower(bound) => (
                bound.bound,
                Type::TypeVar(bound.typevar),
                bound.provenance,
                false,
            ),
            Self::ConcreteUpper(bound) => (
                Type::TypeVar(bound.typevar),
                bound.bound,
                bound.provenance,
                false,
            ),
            Self::ConcreteEquivalence(bound) => (
                bound.bound,
                Type::TypeVar(bound.typevar),
                bound.provenance,
                true,
            ),
            Self::TypeVarRange(bound) => (
                Type::TypeVar(bound.left),
                Type::TypeVar(bound.right),
                bound.provenance,
                false,
            ),
            Self::TypeVarEquivalence(bound) => (
                Type::TypeVar(bound.left),
                Type::TypeVar(bound.right),
                bound.provenance,
                true,
            ),
        };
        let kind = if equivalent {
            RelationKind::Equivalent
        } else {
            RelationKind::Assignable
        };
        [
            Some(ConstraintRelation {
                kind,
                lower,
                upper,
                provenance,
            }),
            equivalent.then_some(ConstraintRelation {
                kind,
                lower: upper,
                upper: lower,
                provenance,
            }),
        ]
        .into_iter()
        .flatten()
    }
}

/// Quotients the path's subtype relations by mutual reachability. Occurrences inside a type
/// constructor are deliberately not edges: equality does not imply structural recursion.
pub(super) struct PathRelations<'db> {
    bindings: ExactBindings<'db>,
    types: FxIndexSet<Type<'db>>,
    components: Vec<usize>,
    supertypes: Vec<FxHashSet<usize>>,
    proofs: PairVisitor<'db, Self, bool>,
    equivalences: PairVisitor<'db, EquivalenceProof, bool>,
    proof_revision: Cell<usize>,
    proof_results: RefCell<FxHashMap<(Type<'db>, Type<'db>), (bool, usize)>>,
}

struct EquivalenceProof;

impl<'db> PathRelations<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &[ConstraintRelation<'db>],
    ) -> Self {
        let mut types = FxIndexSet::default();
        let mut edges = Vec::new();
        for constraint in constraints {
            if constraint.kind == RelationKind::Equivalent
                || (constraint.lower.is_static_sequent_eligible(db, env)
                    && constraint.upper.is_static_sequent_eligible(db, env))
            {
                let lower = types.insert_full(constraint.lower.resolve_type_alias(db)).0;
                let upper = types.insert_full(constraint.upper.resolve_type_alias(db)).0;
                edges.push((lower, upper));
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
            bindings: ExactBindings::new(db, env, constraints.iter().copied()),
            types,
            components,
            supertypes,
            proofs: PairVisitor::new(false),
            equivalences: PairVisitor::new(false),
            proof_revision: Cell::new(0),
            proof_results: RefCell::default(),
        }
    }

    fn component(&self, db: &'db dyn Db, ty: Type<'db>) -> Option<usize> {
        self.types
            .get_index_of(&ty.resolve_type_alias(db))
            .map(|index| self.components[index])
    }

    fn contains_subtype(&self, db: &'db dyn Db, lower: Type<'db>, upper: Type<'db>) -> bool {
        match (self.component(db, lower), self.component(db, upper)) {
            (Some(lower), Some(upper)) => lower == upper || self.supertypes[lower].contains(&upper),
            _ => false,
        }
    }

    /// Reusing an active obligation is not evidence for subtyping. A failed search remains
    /// reusable until another obligation is proved, which can discharge its cyclic dependencies.
    fn prove_subtype(
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

    fn upper_types(
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

    fn lower_types(
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

    /// Compare gradual bounds after substituting the path's acyclic exact bindings.
    pub(super) fn is_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> bool {
        let (lower, _) = self
            .bindings
            .apply(db, env, lower, ConstraintProvenance::Evidence);
        let (upper, _) = self
            .bindings
            .apply(db, env, upper, ConstraintProvenance::Evidence);
        lower.is_constraint_set_assignable_to(db, env, upper)
    }

    /// Prove exact equality structurally under the path's relations.
    pub(super) fn is_equivalent(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> bool {
        if lower == upper
            || (self.contains_subtype(db, lower, upper) && self.contains_subtype(db, upper, lower))
        {
            return true;
        }
        self.equivalences
            .try_visit(
                db,
                (lower, upper),
                |proved| *proved,
                || {
                    if lower.is_type_var() || upper.is_type_var() {
                        return self.types.iter().copied().any(|bound| {
                            !bound.is_type_var()
                                && ((lower.is_type_var()
                                    && self.contains_subtype(db, lower, bound)
                                    && self.contains_subtype(db, bound, lower)
                                    && self.is_equivalent(db, env, bound, upper))
                                    || (upper.is_type_var()
                                        && self.contains_subtype(db, upper, bound)
                                        && self.contains_subtype(db, bound, upper)
                                        && self.is_equivalent(db, env, lower, bound)))
                        });
                    }
                    let builder = ConstraintSetBuilder::new();
                    let relation = lower.when_constraint_set_equivalent_to_with_provenance(
                        db,
                        env,
                        upper,
                        &builder,
                        ConstraintProvenance::Evidence,
                    );
                    self.implies_node(db, env, &builder.storage.borrow(), relation.node)
                },
            )
            .unwrap_or(false)
    }

    /// Prove subtyping structurally, treating type variables as opaque except for path facts.
    pub(super) fn is_subtype(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> bool {
        if !lower.is_static_sequent_eligible(db, env) || !upper.is_static_sequent_eligible(db, env)
        {
            return self.is_equivalent(db, env, lower, upper);
        }
        if self.contains_subtype(db, lower, upper) {
            return true;
        }
        self.prove_subtype(db, lower, upper, || {
            // Only path facts establish variable bounds here. Consulting declarations would
            // discard invalid evidence before the walker can attribute its bound violation.
            if lower.is_type_var() || upper.is_type_var() {
                return (lower.is_type_var()
                    && self
                        .upper_types(db, lower)
                        .any(|bound| self.is_subtype(db, env, bound, upper, builder)))
                    || (upper.is_type_var()
                        && self
                            .lower_types(db, upper)
                            .any(|bound| self.is_subtype(db, env, lower, bound, builder)));
            }
            let relation = lower.when_constraint_set_subtype_of(db, env, upper, builder);
            self.implies_node(db, env, &builder.storage.borrow(), relation.node)
        })
    }

    fn implies_node(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &ConstraintSetStorage<'db>,
        node: NodeId,
    ) -> bool {
        match node.node() {
            Node::AlwaysTrue => true,
            Node::AlwaysFalse => false,
            Node::Interior(_) => {
                let interior = storage.interior_node_data(node);
                if self.implies_node(db, env, storage, interior.if_uncertain) {
                    return true;
                }
                if !self.implies_node(db, env, storage, interior.if_true) {
                    return false;
                }
                let builder = ConstraintSetBuilder::new();
                storage
                    .constraint_data(interior.constraint)
                    .relations()
                    .all(|relation| match relation.kind {
                        RelationKind::Equivalent => {
                            self.is_equivalent(db, env, relation.lower, relation.upper)
                        }
                        RelationKind::Assignable => {
                            self.is_subtype(db, env, relation.lower, relation.upper, &builder)
                        }
                    })
                    || self.implies_node(db, env, storage, interior.if_false)
            }
        }
    }
}
