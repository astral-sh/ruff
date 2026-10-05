//! Shared source-order interning with admission before lookup, growth, and publication.

use std::ops::ControlFlow;

use super::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork, Unrestricted, hash_access,
    reserve_map, reserve_vec, unrestricted,
};
use super::{ConstraintSetStorage, SourceOrder, SourceOrderId};

pub(super) fn next_source_order_id<E>(
    local_len: usize,
    overlay_len: usize,
) -> Result<SourceOrderId, TddError<E>> {
    let index = local_len
        .checked_add(overlay_len)
        .ok_or(TddError::CapacityExhausted)?;
    if index > SourceOrderId::MAX_VALUE as usize {
        return Err(TddError::CapacityExhausted);
    }
    Ok(SourceOrderId::from_usize(index))
}

#[derive(Clone, Copy)]
pub(super) enum OrderedSource {
    Existing(Option<SourceOrderId>),
    Intern(SourceOrder),
}

impl OrderedSource {
    #[inline]
    pub(super) fn new(left: Option<SourceOrderId>, right: Option<SourceOrderId>) -> Self {
        match (left, right) {
            (None, None) => Self::Existing(None),
            (None, other) | (other, None) => Self::Existing(other),
            (Some(left), Some(right)) if left == right => Self::Existing(Some(left)),
            (Some(left), Some(right)) => Self::Intern(SourceOrder::Ordered(left, right)),
        }
    }

    #[inline]
    pub(super) fn finish(self, storage: &mut ConstraintSetStorage<'_>) -> Option<SourceOrderId> {
        match self {
            Self::Existing(result) => result,
            Self::Intern(data) => Some(PendingSourceOrder::new(data).finish(storage)),
        }
    }
}

pub(super) struct PendingSourceOrder {
    data: SourceOrder,
    result: Option<SourceOrderId>,
}

impl PendingSourceOrder {
    pub(super) fn new(data: SourceOrder) -> Self {
        Self { data, result: None }
    }

    pub(super) fn finish(mut self, storage: &mut ConstraintSetStorage<'_>) -> SourceOrderId {
        loop {
            if let ControlFlow::Break(result) =
                unrestricted(self.advance_with(storage, &mut Unrestricted))
            {
                return result;
            }
        }
    }

    /// After refusal, abandon this cursor. A fresh cursor can reuse completed identities and
    /// retained capacity in the same storage; no unfinished sidecar has been published.
    pub(super) fn advance_with<C: TddControl>(
        &mut self,
        storage: &mut ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<SourceOrderId>, TddError<C::Error>> {
        control.admit(TddWork::SourceOrderAdvance)?;
        if let Some(result) = self.result {
            return Ok(ControlFlow::Break(result));
        }
        if storage.advance_identity_caches(control)?.is_continue() {
            return Ok(ControlFlow::Continue(()));
        }
        hash_access(
            control,
            TableKind::SourceOrders,
            storage.source_order_cache.capacity(),
        )?;
        if let Some(result) = storage.source_order_cache.get(&self.data).copied() {
            self.result = Some(result);
            return Ok(ControlFlow::Break(result));
        }
        let result = next_source_order_id::<C::Error>(
            storage.source_orders.len(),
            storage
                .compacted
                .as_ref()
                .map_or(0, |owned| owned.source_orders.len()),
        )?;
        reserve_vec(
            &mut storage.source_orders.raw,
            1,
            AllocationKind::SourceOrders,
            control,
        )?;
        reserve_map(
            &mut storage.source_order_cache,
            TableKind::SourceOrders,
            control,
        )?;
        control.admit(TddWork::SourceOrderCommit)?;
        // No yield or refusal can split the arena append from its identity-cache insertion.
        storage.source_orders.raw.push(self.data);
        storage.source_order_cache.insert(self.data, result);
        self.result = Some(result);
        Ok(ControlFlow::Break(result))
    }
}
