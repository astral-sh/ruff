//! Resumable identity initialization, support merging, and atomic node publication.

use std::ops::ControlFlow;

use super::apply::Operation;
use super::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork, Unrestricted, hash_access,
    reserve_map, reserve_smallvec, reserve_vec, unrestricted,
};
use super::support::{Support, SupportId};
use super::{
    ConstraintId, ConstraintSetStorage, InteriorNodeData, NodeId, SMALLEST_TERMINAL, SourceOrderId,
};

const WORD_BATCH: usize = 64;

pub(super) fn next_node_ids<E>(
    node_count: usize,
    node_offset: usize,
    support_count: usize,
    support_offset: usize,
) -> Result<(NodeId, SupportId), TddError<E>> {
    let node_index = node_count
        .checked_add(node_offset)
        .ok_or(TddError::CapacityExhausted)?;
    let support_index = support_count
        .checked_add(support_offset)
        .ok_or(TddError::CapacityExhausted)?;
    if node_index >= SMALLEST_TERMINAL.0 as usize || support_index > (u32::MAX - 1) as usize {
        return Err(TddError::CapacityExhausted);
    }
    Ok((
        NodeId::from_usize(node_index),
        SupportId::from_usize(support_index),
    ))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum OverlayIdentityState {
    #[default]
    Start,
    Constraints {
        bit_index: usize,
        dense_index: usize,
    },
    Nodes {
        bit_index: usize,
        dense_index: usize,
    },
    SourceOrders {
        index: usize,
    },
    Ready,
}

impl ConstraintSetStorage<'_> {
    pub(super) fn advance_identity_caches<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<()>, TddError<C::Error>> {
        let Some(compacted) = &self.compacted else {
            self.overlay_identity_state = OverlayIdentityState::Ready;
            return Ok(ControlFlow::Break(()));
        };
        match self.overlay_identity_state {
            OverlayIdentityState::Start => {
                self.overlay_identity_state = OverlayIdentityState::Constraints {
                    bit_index: 0,
                    dense_index: 0,
                };
            }
            OverlayIdentityState::Constraints {
                mut bit_index,
                mut dense_index,
            } => {
                for _ in 0..WORD_BATCH {
                    if bit_index == compacted.constraint_indices.len() {
                        self.overlay_identity_state = OverlayIdentityState::Nodes {
                            bit_index: 0,
                            dense_index: 0,
                        };
                        break;
                    }
                    control.admit(TddWork::OverlayScan { slots: 1 })?;
                    if compacted.constraint_indices.get_bit(bit_index) == Some(true) {
                        reserve_map(&mut self.constraint_cache, TableKind::Constraints, control)?;
                        self.constraint_cache.insert(
                            compacted.constraints[dense_index],
                            ConstraintId::from_usize(bit_index),
                        );
                        dense_index += 1;
                    }
                    bit_index += 1;
                    self.overlay_identity_state = OverlayIdentityState::Constraints {
                        bit_index,
                        dense_index,
                    };
                }
            }
            OverlayIdentityState::Nodes {
                mut bit_index,
                mut dense_index,
            } => {
                for _ in 0..WORD_BATCH {
                    if bit_index == compacted.node_indices.len() {
                        self.overlay_identity_state =
                            OverlayIdentityState::SourceOrders { index: 0 };
                        break;
                    }
                    control.admit(TddWork::OverlayScan { slots: 1 })?;
                    if compacted.node_indices.get_bit(bit_index) == Some(true) {
                        reserve_map(&mut self.node_cache, TableKind::Nodes, control)?;
                        self.node_cache
                            .insert(compacted.nodes[dense_index], NodeId::from_usize(bit_index));
                        dense_index += 1;
                    }
                    bit_index += 1;
                    self.overlay_identity_state = OverlayIdentityState::Nodes {
                        bit_index,
                        dense_index,
                    };
                }
            }
            OverlayIdentityState::SourceOrders { mut index } => {
                for _ in 0..WORD_BATCH {
                    if index == compacted.source_orders.len() {
                        self.overlay_identity_state = OverlayIdentityState::Ready;
                        break;
                    }
                    control.admit(TddWork::OverlayScan { slots: 1 })?;
                    reserve_map(
                        &mut self.source_order_cache,
                        TableKind::SourceOrders,
                        control,
                    )?;
                    self.source_order_cache.insert(
                        compacted.source_orders[index],
                        SourceOrderId::from_usize(index),
                    );
                    index += 1;
                    self.overlay_identity_state = OverlayIdentityState::SourceOrders { index };
                }
            }
            OverlayIdentityState::Ready => return Ok(ControlFlow::Break(())),
        }
        Ok(ControlFlow::Continue(()))
    }
}

#[derive(Default)]
struct SupportMerge {
    result: Support,
    sources: [Option<SupportId>; 4],
    max_words: usize,
    reserved: bool,
    source: usize,
    word: usize,
}

impl SupportMerge {
    fn new(storage: &ConstraintSetStorage<'_>, data: InteriorNodeData) -> Self {
        let sources = [
            Some(storage.constraint_support_id(data.constraint)),
            storage.node_support_id(data.if_true),
            storage.node_support_id(data.if_uncertain),
            storage.node_support_id(data.if_false),
        ];
        let max_words = sources
            .iter()
            .flatten()
            .map(|source| storage.support_data(*source).words().len())
            .max()
            .unwrap_or(0);
        Self {
            sources,
            max_words,
            ..Self::default()
        }
    }

    fn advance<C: TddControl>(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<()>, TddError<C::Error>> {
        if !self.reserved {
            reserve_smallvec(
                self.result.words_mut(),
                self.max_words,
                AllocationKind::SupportWords,
                control,
            )?;
            self.reserved = true;
        }
        if self.result.words().len() < self.max_words {
            let words = (self.max_words - self.result.words().len()).min(WORD_BATCH);
            control.admit(TddWork::SupportWords { words })?;
            let len = self.result.words().len() + words;
            self.result.words_mut().resize(len, 0);
            return Ok(ControlFlow::Continue(()));
        }
        if self.source == self.sources.len() {
            return Ok(ControlFlow::Break(()));
        }
        if let Some(source) = self.sources[self.source] {
            let source = storage.support_data(source);
            let words = (source.words().len() - self.word).min(WORD_BATCH);
            control.admit(TddWork::SupportWords { words })?;
            let end = self.word + words;
            for index in self.word..end {
                self.result.words_mut()[index] |= source.words()[index];
            }
            self.word = end;
            if self.word != source.words().len() {
                return Ok(ControlFlow::Continue(()));
            }
            if !source.is_complete() {
                self.result.mark_incomplete();
            }
        }
        self.source += 1;
        self.word = 0;
        Ok(ControlFlow::Continue(()))
    }
}

#[derive(Clone, Copy)]
enum NodePhase {
    Reduce,
    Intern,
    Merge,
    Commit,
    Existing(NodeId),
    Done(NodeId),
}

pub(super) enum ReducedNode {
    Existing(NodeId),
    Interior(InteriorNodeData),
}

pub(super) struct PendingNode {
    data: InteriorNodeData,
    operation: Option<Operation>,
    phase: NodePhase,
    support: SupportMerge,
}

impl PendingNode {
    pub(super) fn new(data: InteriorNodeData, operation: Option<Operation>, reduce: bool) -> Self {
        Self {
            data,
            operation,
            phase: if reduce {
                NodePhase::Reduce
            } else {
                NodePhase::Intern
            },
            support: SupportMerge::default(),
        }
    }

    pub(super) fn finish(mut self, storage: &mut ConstraintSetStorage<'_>) -> NodeId {
        loop {
            if let ControlFlow::Break(node) = unrestricted(self.advance(storage, &mut Unrestricted))
            {
                return node;
            }
        }
    }

    pub(super) fn has_ready_completion_phase(&self, storage: &ConstraintSetStorage<'_>) -> bool {
        match self.phase {
            NodePhase::Reduce | NodePhase::Existing(_) | NodePhase::Done(_) => true,
            NodePhase::Intern => {
                storage.compacted.is_none()
                    || storage.overlay_identity_state == OverlayIdentityState::Ready
            }
            NodePhase::Merge | NodePhase::Commit => false,
        }
    }

    pub(super) fn advance<C: TddControl>(
        &mut self,
        storage: &mut ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<NodeId>, TddError<C::Error>> {
        control.admit(TddWork::Advance)?;
        match self.phase {
            NodePhase::Reduce => {
                control.admit(TddWork::CoverageReduction)?;
                match NodeId::reduce_uncertain(storage, self.data) {
                    ReducedNode::Existing(node) => self.phase = NodePhase::Existing(node),
                    ReducedNode::Interior(data) => {
                        self.data = data;
                        self.phase = NodePhase::Intern;
                    }
                }
            }
            NodePhase::Intern => {
                if storage.advance_identity_caches(control)?.is_continue() {
                    return Ok(ControlFlow::Continue(()));
                }
                hash_access(control, TableKind::Nodes, storage.node_cache.capacity())?;
                if let Some(node) = storage.node_cache.get(&self.data) {
                    self.phase = NodePhase::Existing(*node);
                } else {
                    self.support = SupportMerge::new(storage, self.data);
                    self.phase = NodePhase::Merge;
                }
            }
            NodePhase::Merge => {
                if self.support.advance(storage, control)?.is_break() {
                    self.phase = NodePhase::Commit;
                }
            }
            NodePhase::Commit => {
                // Another operation can intern this identity while its support is being merged.
                hash_access(control, TableKind::Nodes, storage.node_cache.capacity())?;
                if let Some(node) = storage.node_cache.get(&self.data) {
                    self.phase = NodePhase::Existing(*node);
                    return Ok(ControlFlow::Continue(()));
                }
                let node_offset = storage
                    .compacted
                    .as_ref()
                    .map_or(0, |old| old.node_indices.len());
                let support_offset = storage
                    .compacted
                    .as_ref()
                    .map_or(0, |old| old.support_indices.len());
                let (node, support_id) = next_node_ids::<C::Error>(
                    storage.nodes.len(),
                    node_offset,
                    storage.supports.len(),
                    support_offset,
                )?;
                reserve_vec(
                    &mut storage.supports.raw,
                    1,
                    AllocationKind::Supports,
                    control,
                )?;
                reserve_vec(&mut storage.nodes.raw, 1, AllocationKind::Nodes, control)?;
                reserve_vec(
                    &mut storage.node_supports.raw,
                    1,
                    AllocationKind::NodeSupports,
                    control,
                )?;
                reserve_map(&mut storage.node_cache, TableKind::Nodes, control)?;
                if let Some(operation) = self.operation {
                    operation.prepare_cache(storage, control)?;
                }
                control.admit(TddWork::Commit)?;
                let support = std::mem::take(&mut self.support.result);
                storage.supports.push(support);
                storage.nodes.push(self.data);
                storage.node_supports.push(support_id);
                storage.node_cache.insert(self.data, node);
                if let Some(operation) = self.operation {
                    operation.cache(storage, node);
                }
                self.phase = NodePhase::Done(node);
            }
            NodePhase::Existing(node) => {
                if let Some(operation) = self.operation {
                    operation.prepare_cache(storage, control)?;
                    control.admit(TddWork::Commit)?;
                    operation.cache(storage, node);
                }
                self.phase = NodePhase::Done(node);
            }
            NodePhase::Done(node) => return Ok(ControlFlow::Break(node)),
        }
        Ok(ControlFlow::Continue(()))
    }
}
