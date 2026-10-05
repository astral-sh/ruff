//! Shared satisfaction decisions over the caller's original constraint storage.

use std::convert::Infallible;
use std::ops::ControlFlow;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use super::control::{
    AllocationKind, PathAdvance, PathTable, PathTypevarSet, PathWork, Unrestricted,
    admit_path_work, reserve_smallvec, unrestricted,
};
use super::paths::{
    PathAssignments, PathVisitEffects, SyncPathVisitEffects, path_visit_owned_sync,
    path_visit_owned_with, reserve_path_typevars_with,
};
use super::variables::Constraint;
use super::{
    ALWAYS_FALSE, ConstraintId, ConstraintSetStorage, InteriorNode, IsNeverSatisfiedVisitor, Node,
    NodeId, SourceOrderId, SourceOrderScan, TypeVarId, UniqueConstraintScan,
    extend_dependent_support_with,
};
use crate::types::BoundTypeVarInstance;
use crate::{Db, FxIndexSet, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(super) enum SatisfactionKind {
    Never,
    Always,
}

pub(super) fn node_satisfaction_start(
    node: NodeId,
    kind: SatisfactionKind,
) -> ControlFlow<bool, InteriorNode> {
    match node.node() {
        Node::AlwaysTrue => ControlFlow::Break(matches!(kind, SatisfactionKind::Always)),
        Node::AlwaysFalse => ControlFlow::Break(matches!(kind, SatisfactionKind::Never)),
        Node::Interior(interior) => ControlFlow::Continue(interior),
    }
}

pub(super) trait SatisfactionEffects<'db>:
    PathVisitEffects<'db, IsNeverSatisfiedVisitor>
{
    async fn never_cache_get(&mut self, node: NodeId) -> Result<Option<bool>, Self::Error>;
    async fn never_cache_insert(&mut self, node: NodeId, value: bool) -> Result<(), Self::Error>;
    async fn collect_unique_constraints(
        &mut self,
        node: NodeId,
    ) -> Result<SmallVec<[ConstraintId; 8]>, Self::Error>;
    async fn collect_source_order(
        &mut self,
        source_order: Option<SourceOrderId>,
    ) -> Result<FxIndexSet<ConstraintId>, Self::Error>;
    async fn is_single_conjunction(&mut self, node: NodeId) -> Result<bool, Self::Error>;
    async fn as_concrete(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    async fn existing_typevar_id(
        &mut self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarId, Self::Error>;
    async fn extend_dependent_support(
        &mut self,
        constraint: ConstraintId,
        dependent: &mut FxHashSet<TypeVarId>,
    ) -> Result<(), Self::Error>;
    async fn reserve_typevars(
        &mut self,
        set: &mut FxHashSet<TypeVarId>,
        kind: PathTypevarSet,
    ) -> Result<(), Self::Error>;
}

pub(super) trait SyncSatisfactionEffects<'db>:
    SyncPathVisitEffects<'db, IsNeverSatisfiedVisitor>
{
    fn never_cache_get(&mut self, node: NodeId) -> Result<Option<bool>, Self::Error>;
    fn never_cache_insert(&mut self, node: NodeId, value: bool) -> Result<(), Self::Error>;
    fn collect_unique_constraints(
        &mut self,
        node: NodeId,
    ) -> Result<SmallVec<[ConstraintId; 8]>, Self::Error>;
    fn collect_source_order(
        &mut self,
        source_order: Option<SourceOrderId>,
    ) -> Result<FxIndexSet<ConstraintId>, Self::Error>;
    fn is_single_conjunction(&mut self, node: NodeId) -> Result<bool, Self::Error>;
    fn as_concrete(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    fn existing_typevar_id(
        &mut self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarId, Self::Error>;
    fn extend_dependent_support(
        &mut self,
        constraint: ConstraintId,
        dependent: &mut FxHashSet<TypeVarId>,
    ) -> Result<(), Self::Error>;
    fn reserve_typevars(
        &mut self,
        set: &mut FxHashSet<TypeVarId>,
        kind: PathTypevarSet,
    ) -> Result<(), Self::Error>;
}

pub(super) struct OrdinarySatisfaction<'storage, 'env, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) storage: &'storage mut ConstraintSetStorage<'db>,
}

impl<'db> SyncSatisfactionEffects<'db> for OrdinarySatisfaction<'_, '_, 'db> {
    fn never_cache_get(&mut self, node: NodeId) -> Result<Option<bool>, Infallible> {
        unrestricted(admit_path_work(
            PathWork::Access(PathTable::NeverCache),
            &mut Unrestricted,
        ));
        Ok(self.storage.never_satisfied_cache.get(&node).copied())
    }

    fn never_cache_insert(&mut self, node: NodeId, value: bool) -> Result<(), Infallible> {
        unrestricted(admit_path_work(
            PathWork::Access(PathTable::NeverCache),
            &mut Unrestricted,
        ));
        self.storage.never_satisfied_cache.insert(node, value);
        Ok(())
    }

    fn collect_unique_constraints(
        &mut self,
        node: NodeId,
    ) -> Result<SmallVec<[ConstraintId; 8]>, Infallible> {
        let mut scan = UniqueConstraintScan::new(node);
        let mut result = SmallVec::new();
        loop {
            match unrestricted(scan.advance_with(self.storage, &mut Unrestricted)) {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(Some(constraint)) => {
                    unrestricted(reserve_smallvec(
                        &mut result,
                        1,
                        AllocationKind::UniqueConstraintOutput,
                        &mut Unrestricted,
                    ));
                    result.push(constraint);
                }
                ControlFlow::Break(None) => return Ok(result),
            }
        }
    }

    fn collect_source_order(
        &mut self,
        source_order: Option<SourceOrderId>,
    ) -> Result<FxIndexSet<ConstraintId>, Infallible> {
        let mut scan = SourceOrderScan::new(source_order);
        loop {
            if unrestricted(scan.advance_with(self.storage, &mut Unrestricted)).is_break() {
                return Ok(scan.result);
            }
        }
    }

    fn is_single_conjunction(&mut self, node: NodeId) -> Result<bool, Infallible> {
        Ok(node.is_single_conjunction(self.storage))
    }

    fn as_concrete(
        &mut self,
        constraint: Constraint<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(constraint.as_concrete(self.db, self.env))
    }

    fn existing_typevar_id(
        &mut self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarId, Infallible> {
        Ok(self.storage.typevar_id(self.db, typevar))
    }

    fn extend_dependent_support(
        &mut self,
        constraint: ConstraintId,
        dependent: &mut FxHashSet<TypeVarId>,
    ) -> Result<(), Infallible> {
        unrestricted(extend_dependent_support_with(
            self.storage,
            constraint,
            dependent,
            &mut Unrestricted,
        ));
        Ok(())
    }

    fn reserve_typevars(
        &mut self,
        set: &mut FxHashSet<TypeVarId>,
        kind: PathTypevarSet,
    ) -> Result<(), Infallible> {
        unrestricted(reserve_path_typevars_with(set, kind, &mut Unrestricted));
        Ok(())
    }
}

#[ty_mapping_probe_macros::dual_satisfaction]
pub(super) async fn node_satisfaction_with<'db, E: SatisfactionEffects<'db>>(
    node: NodeId,
    source_order: Option<SourceOrderId>,
    kind: SatisfactionKind,
    effects: &mut E,
) -> Result<bool, E::Error> {
    effects
        .checkpoint(PathWork::Advance(PathAdvance::Satisfaction))
        .await?;
    match node_satisfaction_start(node, kind) {
        ControlFlow::Break(result) => Ok(result),
        ControlFlow::Continue(interior) => {
            let never = matches!(kind, SatisfactionKind::Never);
            if never && let Some(result) = effects.never_cache_get(node).await? {
                return Ok(result);
            }
            let result = if never && simple_conjunction_satisfiable_with(node, effects).await? {
                false
            } else {
                let path = path_assignments_with(interior, source_order, effects).await?;
                let mut visitor = IsNeverSatisfiedVisitor;
                path_visit_owned_with(path, node, &mut visitor, !never, effects)
                    .await?
                    .flow
                    .is_continue()
            };
            if never {
                effects.never_cache_insert(node, result).await?;
            }
            Ok(result)
        }
    }
}

/// Checks for one positive lower-bound-only or upper-bound-only conjunction.
/// Such constraints have object or Never, respectively, as a valid solution.
#[ty_mapping_probe_macros::dual_satisfaction]
async fn simple_conjunction_satisfiable_with<'db, E: SatisfactionEffects<'db>>(
    node: NodeId,
    effects: &mut E,
) -> Result<bool, E::Error> {
    let mut node = node;
    let mut found_lower = false;
    let mut found_upper = false;
    loop {
        effects
            .checkpoint(PathWork::Advance(PathAdvance::SimpleConjunction))
            .await?;
        match node.node() {
            Node::AlwaysTrue => return Ok(true),
            Node::AlwaysFalse => return Ok(false),
            Node::Interior(_) => {
                let interior = effects.interior_data(node).await?;
                if interior.if_false != ALWAYS_FALSE || interior.if_uncertain != ALWAYS_FALSE {
                    return Ok(false);
                }
                let constraint = effects.constraint_data(interior.constraint).await?;
                found_lower |= constraint.provides_lower();
                found_upper |= constraint.provides_upper();
                if found_lower && found_upper {
                    return Ok(false);
                }
                node = interior.if_true;
            }
        }
    }
}

#[ty_mapping_probe_macros::dual_satisfaction]
pub(super) async fn path_assignments_with<'db, E: SatisfactionEffects<'db>>(
    interior: InteriorNode,
    source_order: Option<SourceOrderId>,
    effects: &mut E,
) -> Result<PathAssignments, E::Error> {
    let mut constraints = effects.collect_unique_constraints(interior.node()).await?;
    let source_orders = effects.collect_source_order(source_order).await?;
    effects
        .checkpoint(PathWork::SourceOrderSort {
            entries: constraints.len(),
        })
        .await?;
    // `PathAssignments` seeds its insertion-ordered discovered-constraint map from this list,
    // and uses that order when constructing non-commutative sequent pairs. Do not replace this
    // with TDD traversal order: doing so can change inference and lose gradual constraints.
    // Every constraint in the TDD must appear in the sidecar. If an operation introduces new
    // constraints, it must preserve their source orders rather than invent an order here.
    constraints.sort_by_key(|constraint| {
        source_orders
            .get_index_of(constraint)
            .expect("every BDD constraint should have a source-order entry")
    });
    if !effects.is_single_conjunction(interior.node()).await? {
        effects
            .checkpoint(PathWork::SeedDiscovered {
                entries: constraints.len(),
            })
            .await?;
        return Ok(PathAssignments::new(constraints, FxHashSet::default()));
    }
    let mut independent_typevars = FxHashSet::default();
    let mut dependent_typevars = FxHashSet::default();
    for index in 0..constraints.len() {
        let constraint_id = constraints[index];
        effects
            .checkpoint(PathWork::Advance(PathAdvance::Initialization))
            .await?;
        let constraint = effects.constraint_data(constraint_id).await?;
        if let Some(typevar) = effects.as_concrete(constraint).await? {
            let typevar = effects.existing_typevar_id(typevar).await?;
            effects
                .checkpoint(PathWork::Access(PathTable::IndependentTypevars))
                .await?;
            if !independent_typevars.contains(&typevar) {
                effects
                    .reserve_typevars(&mut independent_typevars, PathTypevarSet::Independent)
                    .await?;
                independent_typevars.insert(typevar);
            }
        } else {
            effects
                .extend_dependent_support(constraint_id, &mut dependent_typevars)
                .await?;
        }
    }
    effects
        .checkpoint(PathWork::RetainIndependent {
            candidates: independent_typevars.len(),
            candidate_capacity: independent_typevars.capacity(),
        })
        .await?;
    independent_typevars.retain(|typevar| !dependent_typevars.contains(typevar));
    effects
        .checkpoint(PathWork::SeedDiscovered {
            entries: constraints.len(),
        })
        .await?;
    Ok(PathAssignments::new(constraints, independent_typevars))
}
