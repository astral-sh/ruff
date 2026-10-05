//! Fixed borrowed-node and rank-bit indexing for receiver constraint cursors.

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use ty_python_core::rank::RankBitBox;

use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, CALL_3, CHECKED_MULTIPLY, CHECK_LANGUAGE_UB,
    MAYBE_IS_ALIGNED_AND_NOT_NULL, NONNULL_AS_PTR, NONNULL_CAST, NONNULL_NEW,
    NONNULL_NEW_UNCHECKED_WORK, OVERFLOWING_ADD, POINTER_ADD_WORK,
    RAW_SLICE_POINTER, add_quotes, checked, event_quote,
};
use crate::types::constraints::{ConstraintId, OwnedConstraintTypeCursor};
use crate::types::constraints::control::{AllocationKind, TddControl, TddWork, hash_slots};
use crate::types::constraints::control::attempt::{EndpointAdmission, ExecutionControl};
use crate::types::local_transfer::hash_table;
use crate::types::Type;

// bitvec 1.1.1 and wyz 0.5.1: private spans contain a pointer and a length; the
// public RankBitBox contains both its BitBox span and the boxed chunk-rank slice.
const BITS_OF: usize = CALL_1 + CALL_2 + CHECKED_MULTIPLY + 4;
const INDEX_NEW: usize = CALL_1 + BITS_OF + 7;
const SPAN_FROM_SLICE: usize = 2 * CALL_1 + NONNULL_NEW + NONNULL_CAST
    + CALL_1 + NONNULL_AS_PTR + 14;
const SPAN_TO_SLICE: usize = 3 * CALL_1 + NONNULL_AS_PTR + RAW_SLICE_POINTER + 7;
const SPAN_ADDRESS: usize = 4 * CALL_1 + NONNULL_AS_PTR + 2 * POINTER_ADD_WORK
    + NONNULL_NEW_UNCHECKED_WORK + 12;
const SPAN_HEAD: usize = CALL_1 + NONNULL_AS_PTR + INDEX_NEW + 14;
const BITPTR_NEW: usize = 2 * CALL_2 + 3 * CALL_1 + NONNULL_AS_PTR + 18;
const SPAN_TO_BITPTR: usize = CALL_1 + SPAN_ADDRESS + SPAN_HEAD + BITPTR_NEW + 4;

// Address::offset forwards through with_ptr and three tap::Pipe calls. Its
// successful unwrap retains the non-null pointer; no collection is created.
const ADDRESS_OFFSET: usize = 5 * CALL_2 + 6 * CALL_1 + NONNULL_AS_PTR
    + 2 * POINTER_ADD_WORK + NONNULL_NEW + 16;
const INDEX_OFFSET: usize = CALL_2 + CALL_1 + OVERFLOWING_ADD + INDEX_NEW + 12;
const BITPTR_ADD: usize = 2 * CALL_2 + INDEX_OFFSET + ADDRESS_OFFSET + BITPTR_NEW + 6;
const BIT_SELECT: usize = 6 * CALL_1 + CALL_2 + 18;
const BIT_READ: usize = 5 * CALL_1 + CALL_2 + NONNULL_AS_PTR + BIT_SELECT + 9;
const BITREF_NEW: usize = 4 * CALL_1 + BIT_READ + 13;
const BIT_GET: usize = 2 * CALL_1 + SPAN_TO_SLICE + 2 * CALL_2
    + SPAN_FROM_SLICE + CALL_1 + 3 + CALL_2 + CALL_1 + SPAN_FROM_SLICE
    + SPAN_TO_BITPTR + BITPTR_ADD + BITREF_NEW + 3 * CALL_1 + 20;

const RAW_SLICE: usize = CALL_2 + CHECK_LANGUAGE_UB + CALL_3
    + MAYBE_IS_ALIGNED_AND_NOT_NULL + CHECKED_MULTIPLY + RAW_SLICE_POINTER + 16;
const RAW_BITS: usize = CALL_1 + SPAN_ADDRESS + CALL_1 + NONNULL_AS_PTR
    + CALL_1 + CALL_1 + SPAN_HEAD + CALL_1 + BITS_OF + 11 + RAW_SLICE;
const SLICE_INDEX: usize = 2 * CALL_2 + POINTER_ADD_WORK + 10;
const RANK: usize = CALL_2 + 2 * SLICE_INDEX + CALL_1 + RAW_BITS
    + 2 * CALL_1 + 18;

/// Quotes one receiver step's borrowed-node indexing, fixed hash wrappers and rank-bit operations.
/// Hash-table growth and backing cleanup are quoted separately. The debug
/// membership check reads one bit; rank reads one chunk and its precomputed chunk rank.
pub(super) const fn receiver_index_quote() -> RunResult<(usize, usize)> {
    let access = match hash_table::access_quote::<ConstraintId, ()>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    let fixed = const { event_quote(
        8 * CALL_1 + 5 * CALL_2 + 3 * SLICE_INDEX + BIT_GET + RANK + 46,
        &[
            size_of::<(RankBitBox, usize, usize)>(),
            size_of::<&mut OwnedConstraintTypeCursor<'_, '_>>(),
            size_of::<Option<Option<[Type<'_>; 2]>>>(),
            size_of::<[Type<'_>; 2]>(), size_of::<(*const (), usize, usize)>(),
            size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
        ],
    ) };
    checked(add_quotes(add_quotes(fixed, add_quotes(access, access)), Some((2, 0))))
}

/// Supplements a receiver cursor's existing growth admission with native hash-table operations.
/// The cursor's original control still admits logical payload, relocation and backing cleanup.
pub(super) struct ReceiverControl<'a, 'run, 'db> {
    pub(super) endpoint: &'a TaskEndpoint<'run, 'db>,
}

impl TddControl for ReceiverControl<'_, '_, '_> {
    type Error = RunError;

    fn admit(&mut self, work: TddWork) -> RunResult<()> {
        if let TddWork::Grow { allocation: AllocationKind::TypeWalkReceiverSeen, plan } = work {
            admit_quote(self.endpoint, const { receiver_growth_preparation_quote() })?;
            let len = plan.relocation_units;
            let old_slots = if len == 0 { 0 } else {
                hash_slots::<RunError>(len)
                    .map_err(|_| RunError::Contract("receiver old table quotation overflow"))?
            };
            let capacity = plan.requested_capacity.checked_mul(2)
                .ok_or(RunError::Contract("receiver replacement quotation overflow"))?;
            let new_slots = hash_slots::<RunError>(capacity)
                .map_err(|_| RunError::Contract("receiver replacement quotation overflow"))?;
            let (native_work, bytes) = hash_table::growth_quote::<ConstraintId, ()>(old_slots, new_slots, len)?;
            let native_work = native_work.checked_add(len)
                .ok_or(RunError::Contract("receiver rehash quotation overflow"))?;
            admit_quote(self.endpoint, Ok((native_work, bytes)))?;
        }
        ExecutionControl::new(&EndpointAdmission(self.endpoint)).admit(work)
    }
}

fn admit_quote(endpoint: &TaskEndpoint<'_, '_>, quote: RunResult<(usize, usize)>) -> RunResult<()> {
    let (work, requested_bytes) = quote?;
    endpoint.admit_work(work)?;
    endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
    endpoint.check_completion()
}

const fn receiver_growth_preparation_quote() -> RunResult<(usize, usize)> {
    let hash = match hash_table::preparation_quote() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    checked(add_quotes(hash, event_quote(16 * CALL_2 + 12 * CALL_1 + 38, &[
        size_of::<usize>(), size_of::<Option<usize>>(), size_of::<RunResult<(usize, usize)>>(),
    ])))
}
