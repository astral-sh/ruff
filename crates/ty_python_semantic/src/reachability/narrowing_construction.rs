//! Ordered construction of projected narrowing graphs with a flat continuation stack.
//!
//! Projection, graph OR, cofactor absorption and checkpoint expansion share one work list.
//! Predicate inference and canonical checkpoint queries remain separate semantic effects.

use std::cmp::Ordering;
use std::convert::Infallible;

use ruff_index::Idx;
use smallvec::{SmallVec, smallvec};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::Truthiness;
use ty_python_core::narrowing_constraints::{InteriorNode, ScopedNarrowingConstraint};
use ty_python_core::predicate::{Predicate, PredicateNode, ScopedPredicateId};

use super::{
    NARROWING_EVALUATION_CHECKPOINT_INTERVAL, NarrowingProjector, ProjectedNarrowingCheckpoint,
    ProjectedNarrowingEntry, ProjectedNarrowingNode, ProjectedNarrowingNodeId, analyze_single,
    evaluate_projected_narrowing_checkpoint, predicate_scope,
};
use crate::types::{NarrowingConstraint, Type};

pub(crate) struct NarrowingConstructionFacts;
pub(super) struct OrdinaryNarrowingConstructionEffects;

#[derive(Clone, Copy)]
pub(crate) struct ProjectionPolicy {
    root: ScopedNarrowingConstraint,
    use_root_checkpoint: bool,
}

enum Absorption {
    Collapsed(ProjectedNarrowingNodeId),
    Rewrite(ProjectedNarrowingNode),
    Intern,
}

/// Pending operations own only flat payloads, never other continuations or futures.
pub(crate) enum Frame<'db> {
    Project {
        root: ScopedNarrowingConstraint,
        use_root_checkpoint: bool,
    },
    ReturnProjection(ScopedNarrowingConstraint),
    Visit {
        id: ScopedNarrowingConstraint,
        policy: ProjectionPolicy,
    },
    AnalyzeGate {
        id: ScopedNarrowingConstraint,
        policy: ProjectionPolicy,
    },
    FinishGate {
        id: ScopedNarrowingConstraint,
        branch: ScopedNarrowingConstraint,
    },
    FinishPredicate(ScopedNarrowingConstraint),
    PublishProjection(ScopedNarrowingConstraint),
    PublishPredicate {
        id: ScopedNarrowingConstraint,
        positive: Option<NarrowingConstraint<'db>>,
        negative: Option<NarrowingConstraint<'db>>,
    },
    /// Combines two paths without copying one path into both outcomes of the other's predicate.
    Or(ProjectedNarrowingNodeId, ProjectedNarrowingNodeId),
    OrWithUncertain(ProjectedNarrowingNodeId),
    RetryOrLeft(ProjectedNarrowingNodeId),
    RetryOrRight(ProjectedNarrowingNodeId),
    AfterOrTrue {
        atom: ScopedPredicateId,
        left_uncertain: ProjectedNarrowingNodeId,
        right_uncertain: ProjectedNarrowingNodeId,
        left_false: ProjectedNarrowingNodeId,
        right_false: ProjectedNarrowingNodeId,
    },
    AfterOrUncertain {
        atom: ScopedPredicateId,
        left_false: ProjectedNarrowingNodeId,
        right_false: ProjectedNarrowingNodeId,
        if_true: ProjectedNarrowingNodeId,
    },
    AfterOrFalse {
        atom: ScopedPredicateId,
        if_true: ProjectedNarrowingNodeId,
        if_uncertain: ProjectedNarrowingNodeId,
    },
    AddWithUncertain(ProjectedNarrowingNode),
    PublishOr((ProjectedNarrowingNodeId, ProjectedNarrowingNodeId)),
    /// Interns a projected node, collapsing nodes with identical branches.
    Add(ProjectedNarrowingNode),
    AfterTrueCofactor(ProjectedNarrowingNode),
    AfterFalseCofactor {
        node: ProjectedNarrowingNode,
        when_true: ProjectedNarrowingNodeId,
    },
    /// Expands a deferred suffix when canonicalizing a join requires its predicates.
    ExpandCheckpoint {
        id: ProjectedNarrowingNodeId,
        constraint: ScopedNarrowingConstraint,
    },
}

pub(crate) struct Construction<'db> {
    pub(crate) frames: SmallVec<[Frame<'db>; 8]>,
}

shared_semantic_family! {
    #[synchronous(SynchronousNarrowingConstructionEffects)]
    pub(crate) trait NarrowingConstructionEffects<'db> {
        type Error;

        // A controlled provider must admit frame growth and the eventual retirement of every
        // retained payload before mutation, including payloads held when a child refuses.
        #[operation(local)]
        async fn start(&self, initial: Frame<'db>) -> Result<Construction<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, construction: &mut Construction<'db>) -> Result<Option<Frame<'db>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, construction: &mut Construction<'db>, frame: Frame<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, construction: Construction<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn retire_predicate_constraints(&self, positive: Option<NarrowingConstraint<'db>>, negative: Option<NarrowingConstraint<'db>>) -> Result<(), Self::Error>;

        #[operation(local)]
        async fn has_projection(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedNarrowingConstraint) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn cached_projection(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedNarrowingConstraint) -> Result<Option<ProjectedNarrowingNodeId>, Self::Error>;
        #[operation(local)]
        async fn projected_node(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedNarrowingConstraint) -> Result<ProjectedNarrowingNodeId, Self::Error>;
        #[operation(local)]
        async fn publish_projection(&self, projector: &mut NarrowingProjector<'_, 'db>, id: ScopedNarrowingConstraint, projected: ProjectedNarrowingNodeId) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn remove_projection(&self, projector: &mut NarrowingProjector<'_, 'db>, id: ScopedNarrowingConstraint) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn constraint_node(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedNarrowingConstraint) -> Result<InteriorNode, Self::Error>;
        #[operation(local)]
        async fn predicate(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedPredicateId) -> Result<Predicate<'db>, Self::Error>;
        #[operation(local)]
        async fn targets_place(&self, projector: &NarrowingProjector<'_, 'db>, id: ScopedPredicateId) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn graph_node(&self, projector: &NarrowingProjector<'_, 'db>, id: ProjectedNarrowingNodeId) -> Result<ProjectedNarrowingEntry<'db>, Self::Error>;
        #[operation(local)]
        async fn cached_or(&self, projector: &NarrowingProjector<'_, 'db>, key: (ProjectedNarrowingNodeId, ProjectedNarrowingNodeId)) -> Result<Option<ProjectedNarrowingNodeId>, Self::Error>;
        #[operation(local)]
        async fn publish_or(&self, projector: &mut NarrowingProjector<'_, 'db>, key: (ProjectedNarrowingNodeId, ProjectedNarrowingNodeId), projected: ProjectedNarrowingNodeId) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn cached_node(&self, projector: &NarrowingProjector<'_, 'db>, node: ProjectedNarrowingNode) -> Result<Option<ProjectedNarrowingNodeId>, Self::Error>;

        // Appending an entry must admit the complete transaction before changing any vector.
        // Predicate insertion also covers node-cache growth and all three edge references.
        #[operation(local)]
        async fn append_checkpoint(&self, projector: &mut NarrowingProjector<'_, 'db>, constraint: ScopedNarrowingConstraint, ty: Type<'db>) -> Result<ProjectedNarrowingNodeId, Self::Error>;
        #[operation(local)]
        async fn append_predicate(&self, projector: &mut NarrowingProjector<'_, 'db>, node: ProjectedNarrowingNode) -> Result<ProjectedNarrowingNodeId, Self::Error>;

        // These effects retain the canonical semantic producers and their query identities.
        // They require complete admitted descendants or explicit refusal in a controlled run.
        #[operation(source)]
        async fn checkpoint(&self, projector: &NarrowingProjector<'_, 'db>, predicate: Predicate<'db>, constraint: ScopedNarrowingConstraint) -> Result<ProjectedNarrowingCheckpoint<'db>, Self::Error>;
        #[operation(source)]
        async fn analyze(&self, projector: &NarrowingProjector<'_, 'db>, predicate: Predicate<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn predicate_constraints(&self, projector: &mut NarrowingProjector<'_, 'db>, id: ScopedPredicateId) -> Result<(Option<NarrowingConstraint<'db>>, Option<NarrowingConstraint<'db>>), Self::Error>;
    }

    #[finite_capability]
    impl NarrowingConstructionFacts {
        fn false_node(&self) -> ProjectedNarrowingNodeId {
            ProjectedNarrowingNodeId::ALWAYS_FALSE
        }

        fn true_node(&self) -> ProjectedNarrowingNodeId {
            ProjectedNarrowingNodeId::ALWAYS_TRUE
        }

        fn terminal(&self, id: ScopedNarrowingConstraint) -> bool {
            id.is_terminal()
        }

        fn same(&self, left: ProjectedNarrowingNodeId, right: ProjectedNarrowingNodeId) -> bool {
            left == right
        }

        fn checkpoint_position(&self, id: ScopedNarrowingConstraint, atom: ScopedPredicateId, policy: ProjectionPolicy) -> bool {
            let index = atom.index();
            let checkpoint_position = index ^ (index / NARROWING_EVALUATION_CHECKPOINT_INTERVAL);
            (id != policy.root || policy.use_root_checkpoint)
                && (checkpoint_position + 1).is_multiple_of(NARROWING_EVALUATION_CHECKPOINT_INTERVAL)
        }

        fn checkpoint_gate(&self, predicate: Predicate<'_>) -> bool {
            matches!(predicate.node, PredicateNode::ContextManagerSuppresses { .. } | PredicateNode::FinallyNormalPathImpossible { .. })
        }

        fn control_flow_gate(&self, predicate: Predicate<'_>) -> bool {
            matches!(predicate.node, PredicateNode::IsNonTerminalCall(_) | PredicateNode::ContextManagerSuppresses { .. } | PredicateNode::FinallyNormalPathImpossible { .. })
        }

        fn gate_branch(&self, node: InteriorNode, truthiness: Truthiness) -> ScopedNarrowingConstraint {
            match truthiness {
                Truthiness::AlwaysTrue => node.if_true,
                Truthiness::AlwaysFalse => node.if_false,
                Truthiness::Ambiguous => unreachable!("statically decidable predicates should never be Ambiguous"),
            }
        }

        fn has_constraints<'db>(&self, positive: &Option<NarrowingConstraint<'db>>, negative: &Option<NarrowingConstraint<'db>>) -> bool {
            positive.is_some() || negative.is_some()
        }

        fn trivial_or(&self, left: ProjectedNarrowingNodeId, right: ProjectedNarrowingNodeId) -> Option<ProjectedNarrowingNodeId> {
            if left == right || left == ProjectedNarrowingNodeId::ALWAYS_TRUE {
                Some(left)
            } else if right == ProjectedNarrowingNodeId::ALWAYS_TRUE {
                Some(right)
            } else if left == ProjectedNarrowingNodeId::ALWAYS_FALSE {
                Some(right)
            } else if right == ProjectedNarrowingNodeId::ALWAYS_FALSE {
                Some(left)
            } else {
                None
            }
        }

        fn trivial_cofactors(&self, node: ProjectedNarrowingNode) -> Option<(ProjectedNarrowingNodeId, ProjectedNarrowingNodeId)> {
            let when_true = self.trivial_or(node.if_true, node.if_uncertain)?;
            let when_false = self.trivial_or(node.if_false, node.if_uncertain)?;
            Some((when_true, when_false))
        }

        fn or_key(&self, left: ProjectedNarrowingNodeId, right: ProjectedNarrowingNodeId) -> (ProjectedNarrowingNodeId, ProjectedNarrowingNodeId) {
            if left.0 <= right.0 { (left, right) } else { (right, left) }
        }

        fn atom_order(&self, left: ProjectedNarrowingNode, right: ProjectedNarrowingNode) -> Ordering {
            left.atom.cmp(&right.atom).reverse()
        }

        fn trivial_node(&self, node: ProjectedNarrowingNode) -> Option<ProjectedNarrowingNodeId> {
            if node.if_uncertain == ProjectedNarrowingNodeId::ALWAYS_TRUE {
                Some(ProjectedNarrowingNodeId::ALWAYS_TRUE)
            } else if node.if_true == node.if_false && node.if_true == node.if_uncertain {
                Some(node.if_true)
            } else {
                None
            }
        }

        fn absorption(&self, node: ProjectedNarrowingNode, when_true: ProjectedNarrowingNodeId, when_false: ProjectedNarrowingNodeId) -> Absorption {
            if when_true == when_false {
                Absorption::Collapsed(when_true)
            } else if when_true == ProjectedNarrowingNodeId::ALWAYS_TRUE
                && !(node.if_true == ProjectedNarrowingNodeId::ALWAYS_TRUE
                    && node.if_false == ProjectedNarrowingNodeId::ALWAYS_FALSE)
            {
                Absorption::Rewrite(ProjectedNarrowingNode {
                    atom: node.atom,
                    if_true: ProjectedNarrowingNodeId::ALWAYS_TRUE,
                    if_uncertain: when_false,
                    if_false: ProjectedNarrowingNodeId::ALWAYS_FALSE,
                })
            } else if when_false == ProjectedNarrowingNodeId::ALWAYS_TRUE
                && !(node.if_true == ProjectedNarrowingNodeId::ALWAYS_FALSE
                    && node.if_false == ProjectedNarrowingNodeId::ALWAYS_TRUE)
            {
                Absorption::Rewrite(ProjectedNarrowingNode {
                    atom: node.atom,
                    if_true: ProjectedNarrowingNodeId::ALWAYS_FALSE,
                    if_uncertain: when_true,
                    if_false: ProjectedNarrowingNodeId::ALWAYS_TRUE,
                })
            } else {
                Absorption::Intern
            }
        }
    }

    #[synchronous(build_sync)]
    #[capabilities(effects = NarrowingConstructionEffects, facts = NarrowingConstructionFacts)]
    #[passive_values(ProjectionPolicy, ProjectedNarrowingNode, Frame::Project, Frame::ReturnProjection, Frame::Visit, Frame::AnalyzeGate, Frame::FinishGate, Frame::FinishPredicate, Frame::PublishProjection, Frame::PublishPredicate, Frame::Or, Frame::OrWithUncertain, Frame::RetryOrLeft, Frame::RetryOrRight, Frame::AfterOrTrue, Frame::AfterOrUncertain, Frame::AfterOrFalse, Frame::AddWithUncertain, Frame::PublishOr, Frame::Add, Frame::AfterTrueCofactor, Frame::AfterFalseCofactor, Frame::ExpandCheckpoint)]
    pub(crate) async fn build_with<'db, E: NarrowingConstructionEffects<'db>>(
        projector: &mut NarrowingProjector<'_, 'db>,
        initial: Frame<'db>,
        facts: NarrowingConstructionFacts,
        effects: &E,
    ) -> Result<ProjectedNarrowingNodeId, E::Error> {
        let mut construction = effects.start(initial).await?;
        // Every result-consuming continuation follows the operation that produces that result.
        #[passive_state]
        let mut projected = facts.false_node();

        #[cursor_loop]
        while let Some(frame) = effects.next(&mut construction).await? {
            match frame {
                Frame::Project { root, use_root_checkpoint } => {
                    let policy = ProjectionPolicy { root, use_root_checkpoint };
                    effects.push(&mut construction, Frame::ReturnProjection(root)).await?;
                    effects.push(&mut construction, Frame::Visit { id: root, policy }).await?;
                }
                Frame::ReturnProjection(root) => {
                    projected = effects.projected_node(projector, root).await?;
                }
                Frame::Visit { id, policy } => {
                    if facts.terminal(id) || effects.has_projection(projector, id).await? {
                        continue;
                    }

                    let node = effects.constraint_node(projector, id).await?;
                    let predicate = effects.predicate(projector, node.atom).await?;
                    if facts.checkpoint_position(id, node.atom, policy)
                        && (effects.targets_place(projector, node.atom).await? || facts.checkpoint_gate(predicate))
                    {
                        let checkpoint = effects.checkpoint(projector, predicate, id).await?;
                        projected = match checkpoint {
                            ProjectedNarrowingCheckpoint::Unreachable => facts.false_node(),
                            ProjectedNarrowingCheckpoint::Unconstrained => facts.true_node(),
                            ProjectedNarrowingCheckpoint::Narrowed(ty) => effects.append_checkpoint(projector, id, ty).await?,
                        };
                        effects.publish_projection(projector, id, projected).await?;
                        continue;
                    }

                    if facts.control_flow_gate(predicate) {
                        effects.push(&mut construction, Frame::AnalyzeGate { id, policy }).await?;
                        if !facts.terminal(node.if_uncertain) {
                            effects.push(&mut construction, Frame::Visit { id: node.if_uncertain, policy }).await?;
                        }
                    } else {
                        effects.push(&mut construction, Frame::FinishPredicate(id)).await?;
                        if !facts.terminal(node.if_false) {
                            effects.push(&mut construction, Frame::Visit { id: node.if_false, policy }).await?;
                        }
                        if !facts.terminal(node.if_uncertain) {
                            effects.push(&mut construction, Frame::Visit { id: node.if_uncertain, policy }).await?;
                        }
                        if !facts.terminal(node.if_true) {
                            effects.push(&mut construction, Frame::Visit { id: node.if_true, policy }).await?;
                        }
                    }
                }
                Frame::AnalyzeGate { id, policy } => {
                    let node = effects.constraint_node(projector, id).await?;
                    let predicate = effects.predicate(projector, node.atom).await?;
                    let truthiness = effects.analyze(projector, predicate).await?;
                    let branch = facts.gate_branch(node, truthiness);
                    effects.push(&mut construction, Frame::FinishGate { id, branch }).await?;
                    if !facts.terminal(branch) {
                        effects.push(&mut construction, Frame::Visit { id: branch, policy }).await?;
                    }
                }
                Frame::FinishGate { id, branch } => {
                    let node = effects.constraint_node(projector, id).await?;
                    let branch = effects.projected_node(projector, branch).await?;
                    let if_uncertain = effects.projected_node(projector, node.if_uncertain).await?;
                    effects.push(&mut construction, Frame::PublishProjection(id)).await?;
                    effects.push(&mut construction, Frame::Or(branch, if_uncertain)).await?;
                }
                Frame::FinishPredicate(id) => {
                    // Resolve all children after their visits: a later visit can expand an
                    // earlier child's checkpoint and replace its projection-cache entry.
                    let node = effects.constraint_node(projector, id).await?;
                    let if_true = effects.projected_node(projector, node.if_true).await?;
                    let if_uncertain = effects.projected_node(projector, node.if_uncertain).await?;
                    let if_false = effects.projected_node(projector, node.if_false).await?;
                    let (positive, negative) = effects.predicate_constraints(projector, node.atom).await?;
                    let has_constraints = facts.has_constraints(&positive, &negative);
                    effects.push(&mut construction, Frame::PublishPredicate { id, positive, negative }).await?;

                    if has_constraints {
                        effects.push(&mut construction, Frame::Add(ProjectedNarrowingNode { atom: node.atom, if_true, if_uncertain, if_false })).await?;
                    } else {
                        // This node represents `if_uncertain || (P && if_true) || (!P && if_false)`.
                        // Since the predicate `P` cannot narrow this place, remove it while retaining only branches that `P` can take.
                        // Including a statically unreachable branch could erase narrowing from the reachable branch.
                        let predicate = effects.predicate(projector, node.atom).await?;
                        match effects.analyze(projector, predicate).await? {
                            Truthiness::AlwaysTrue => effects.push(&mut construction, Frame::Or(if_true, if_uncertain)).await?,
                            Truthiness::AlwaysFalse => effects.push(&mut construction, Frame::Or(if_false, if_uncertain)).await?,
                            Truthiness::Ambiguous => {
                                effects.push(&mut construction, Frame::OrWithUncertain(if_uncertain)).await?;
                                effects.push(&mut construction, Frame::Or(if_true, if_false)).await?;
                            }
                        }
                    }
                }
                Frame::PublishProjection(id) => {
                    effects.publish_projection(projector, id, projected).await?;
                }
                Frame::PublishPredicate { id, positive, negative } => {
                    effects.publish_projection(projector, id, projected).await?;
                    effects.retire_predicate_constraints(positive, negative).await?;
                }
                Frame::OrWithUncertain(if_uncertain) => {
                    effects.push(&mut construction, Frame::Or(projected, if_uncertain)).await?;
                }
                Frame::Or(left, right) => {
                    if let Some(result) = facts.trivial_or(left, right) {
                        projected = result;
                        continue;
                    }
                    let key = facts.or_key(left, right);
                    if let Some(result) = effects.cached_or(projector, key).await? {
                        projected = result;
                        continue;
                    }

                    let left_entry = effects.graph_node(projector, left).await?;
                    let right_entry = effects.graph_node(projector, right).await?;
                    let (left_node, right_node) = match (left_entry, right_entry) {
                        (ProjectedNarrowingEntry::Checkpoint { constraint, .. }, _) => {
                            // Expansion retries with the original operand order and does not
                            // publish the OR key containing the opaque checkpoint.
                            effects.push(&mut construction, Frame::RetryOrLeft(right)).await?;
                            effects.push(&mut construction, Frame::ExpandCheckpoint { id: left, constraint }).await?;
                            continue;
                        }
                        (_, ProjectedNarrowingEntry::Checkpoint { constraint, .. }) => {
                            effects.push(&mut construction, Frame::RetryOrRight(left)).await?;
                            effects.push(&mut construction, Frame::ExpandCheckpoint { id: right, constraint }).await?;
                            continue;
                        }
                        (ProjectedNarrowingEntry::Predicate(left_node), ProjectedNarrowingEntry::Predicate(right_node)) => (left_node, right_node),
                    };

                    effects.push(&mut construction, Frame::PublishOr(key)).await?;
                    match facts.atom_order(left_node, right_node) {
                        Ordering::Equal => {
                            effects.push(&mut construction, Frame::AfterOrTrue {
                                atom: left_node.atom,
                                left_uncertain: left_node.if_uncertain,
                                right_uncertain: right_node.if_uncertain,
                                left_false: left_node.if_false,
                                right_false: right_node.if_false,
                            }).await?;
                            effects.push(&mut construction, Frame::Or(left_node.if_true, right_node.if_true)).await?;
                        }
                        Ordering::Less => {
                            effects.push(&mut construction, Frame::AddWithUncertain(left_node)).await?;
                            effects.push(&mut construction, Frame::Or(left_node.if_uncertain, right)).await?;
                        }
                        Ordering::Greater => {
                            effects.push(&mut construction, Frame::AddWithUncertain(right_node)).await?;
                            effects.push(&mut construction, Frame::Or(left, right_node.if_uncertain)).await?;
                        }
                    }
                }
                Frame::RetryOrLeft(right) => {
                    effects.push(&mut construction, Frame::Or(projected, right)).await?;
                }
                Frame::RetryOrRight(left) => {
                    effects.push(&mut construction, Frame::Or(left, projected)).await?;
                }
                Frame::AfterOrTrue { atom, left_uncertain, right_uncertain, left_false, right_false } => {
                    effects.push(&mut construction, Frame::AfterOrUncertain { atom, left_false, right_false, if_true: projected }).await?;
                    effects.push(&mut construction, Frame::Or(left_uncertain, right_uncertain)).await?;
                }
                Frame::AfterOrUncertain { atom, left_false, right_false, if_true } => {
                    effects.push(&mut construction, Frame::AfterOrFalse { atom, if_true, if_uncertain: projected }).await?;
                    effects.push(&mut construction, Frame::Or(left_false, right_false)).await?;
                }
                Frame::AfterOrFalse { atom, if_true, if_uncertain } => {
                    effects.push(&mut construction, Frame::Add(ProjectedNarrowingNode { atom, if_true, if_uncertain, if_false: projected })).await?;
                }
                Frame::AddWithUncertain(node) => {
                    effects.push(&mut construction, Frame::Add(ProjectedNarrowingNode { atom: node.atom, if_true: node.if_true, if_uncertain: projected, if_false: node.if_false })).await?;
                }
                Frame::PublishOr(key) => {
                    effects.publish_or(projector, key, projected).await?;
                }
                Frame::Add(node) => {
                    if let Some(result) = facts.trivial_node(node) {
                        projected = result;
                        continue;
                    }

                    // Find and absorb cofactors if we can. (See `ty_python_core::narrowing_constraints` for
                    // more details.)
                    // `if_uncertain` contributes to both cofactors. If either cofactor is already true,
                    // then the remaining cofactor can be lifted into `if_uncertain`, avoiding shapes like
                    // `A or (not A and B)`.
                    if let Some((when_true, when_false)) = facts.trivial_cofactors(node) {
                        projected = when_false;
                        effects.push(&mut construction, Frame::AfterFalseCofactor { node, when_true }).await?;
                    } else {
                        effects.push(&mut construction, Frame::AfterTrueCofactor(node)).await?;
                        effects.push(&mut construction, Frame::Or(node.if_true, node.if_uncertain)).await?;
                    }
                }
                Frame::AfterTrueCofactor(node) => {
                    effects.push(&mut construction, Frame::AfterFalseCofactor { node, when_true: projected }).await?;
                    effects.push(&mut construction, Frame::Or(node.if_false, node.if_uncertain)).await?;
                }
                Frame::AfterFalseCofactor { node, when_true } => {
                    match facts.absorption(node, when_true, projected) {
                        Absorption::Collapsed(result) => {
                            projected = result;
                        }
                        Absorption::Rewrite(node) => effects.push(&mut construction, Frame::Add(node)).await?,
                        Absorption::Intern => {
                            projected = match effects.cached_node(projector, node).await? {
                                Some(result) => result,
                                None => effects.append_predicate(projector, node).await?,
                            };
                        }
                    }
                }
                Frame::ExpandCheckpoint { id, constraint } => {
                    // Keeping checkpoints opaque during evaluation avoids repeated work. During projection,
                    // however, inspecting their predicates lets complementary branches cancel before `TypeGuard`
                    // replacement or ordinary narrowing is applied.
                    if let Some(cached) = effects.cached_projection(projector, constraint).await?
                        && !facts.same(cached, id)
                    {
                        projected = cached;
                        continue;
                    }
                    effects.remove_projection(projector, constraint).await?;
                    effects.push(&mut construction, Frame::Project { root: constraint, use_root_checkpoint: false }).await?;
                }
            }
        }

        effects.finish(construction).await?;
        Ok(projected)
    }
}

impl<'db> SynchronousNarrowingConstructionEffects<'db> for OrdinaryNarrowingConstructionEffects {
    type Error = Infallible;

    fn start(&self, initial: Frame<'db>) -> Result<Construction<'db>, Self::Error> {
        Ok(Construction {
            frames: smallvec![initial],
        })
    }

    fn next(
        &self,
        construction: &mut Construction<'db>,
    ) -> Result<Option<Frame<'db>>, Self::Error> {
        Ok(construction.frames.pop())
    }

    fn push(
        &self,
        construction: &mut Construction<'db>,
        frame: Frame<'db>,
    ) -> Result<(), Self::Error> {
        construction.frames.push(frame);
        Ok(())
    }

    fn finish(&self, construction: Construction<'db>) -> Result<(), Self::Error> {
        drop(construction);
        Ok(())
    }

    fn retire_predicate_constraints(
        &self,
        positive: Option<NarrowingConstraint<'db>>,
        negative: Option<NarrowingConstraint<'db>>,
    ) -> Result<(), Self::Error> {
        drop(negative);
        drop(positive);
        Ok(())
    }

    fn has_projection(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> Result<bool, Self::Error> {
        Ok(projector
            .project_cache
            .contains_key(&(id, projector.base_ty)))
    }

    fn cached_projection(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> Result<Option<ProjectedNarrowingNodeId>, Self::Error> {
        Ok(projector
            .project_cache
            .get(&(id, projector.base_ty))
            .copied())
    }

    fn projected_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> Result<ProjectedNarrowingNodeId, Self::Error> {
        Ok(projector.projected_node(id))
    }

    fn publish_projection(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
        projected: ProjectedNarrowingNodeId,
    ) -> Result<(), Self::Error> {
        projector
            .project_cache
            .insert((id, projector.base_ty), projected);
        Ok(())
    }

    fn remove_projection(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> Result<(), Self::Error> {
        projector.project_cache.remove(&(id, projector.base_ty));
        Ok(())
    }

    fn constraint_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> Result<InteriorNode, Self::Error> {
        Ok(projector.constraints.get_interior_node(id))
    }

    fn predicate(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<Predicate<'db>, Self::Error> {
        Ok(projector.predicates[id])
    }

    fn targets_place(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<bool, Self::Error> {
        Ok(projector
            .predicate_narrowing_targets
            .contains(id, projector.place))
    }

    fn graph_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> Result<ProjectedNarrowingEntry<'db>, Self::Error> {
        Ok(projector.graph.node(id))
    }

    fn cached_or(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        key: (ProjectedNarrowingNodeId, ProjectedNarrowingNodeId),
    ) -> Result<Option<ProjectedNarrowingNodeId>, Self::Error> {
        Ok(projector.graph.or_cache.get(&key).copied())
    }

    fn publish_or(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        key: (ProjectedNarrowingNodeId, ProjectedNarrowingNodeId),
        projected: ProjectedNarrowingNodeId,
    ) -> Result<(), Self::Error> {
        projector.graph.or_cache.insert(key, projected);
        Ok(())
    }

    fn cached_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        node: ProjectedNarrowingNode,
    ) -> Result<Option<ProjectedNarrowingNodeId>, Self::Error> {
        Ok(projector.graph.node_cache.get(&node).copied())
    }

    fn append_checkpoint(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        ty: Type<'db>,
    ) -> Result<ProjectedNarrowingNodeId, Self::Error> {
        let id = ProjectedNarrowingNodeId(projector.graph.nodes.len());
        projector
            .graph
            .nodes
            .push(ProjectedNarrowingEntry::Checkpoint { constraint, ty });
        projector.graph.referenced.push(false);
        projector.graph.joins.push(false);
        Ok(id)
    }

    fn append_predicate(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        node: ProjectedNarrowingNode,
    ) -> Result<ProjectedNarrowingNodeId, Self::Error> {
        let id = ProjectedNarrowingNodeId(projector.graph.nodes.len());
        projector
            .graph
            .nodes
            .push(ProjectedNarrowingEntry::Predicate(node));
        projector.graph.referenced.push(false);
        projector.graph.joins.push(false);
        projector.graph.node_cache.insert(node, id);
        for next in [node.if_true, node.if_uncertain, node.if_false] {
            projector.graph.record_reference(next);
        }
        Ok(id)
    }

    fn checkpoint(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        predicate: Predicate<'db>,
        constraint: ScopedNarrowingConstraint,
    ) -> Result<ProjectedNarrowingCheckpoint<'db>, Self::Error> {
        Ok(evaluate_projected_narrowing_checkpoint(
            projector.db,
            predicate_scope(projector.db, &predicate),
            projector.place,
            constraint,
            projector.base_ty,
        ))
    }

    fn analyze(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        predicate: Predicate<'db>,
    ) -> Result<Truthiness, Self::Error> {
        Ok(analyze_single(projector.db, projector.env, &predicate))
    }

    fn predicate_constraints(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> Result<
        (
            Option<NarrowingConstraint<'db>>,
            Option<NarrowingConstraint<'db>>,
        ),
        Self::Error,
    > {
        Ok(projector.predicate_constraints(id))
    }
}
