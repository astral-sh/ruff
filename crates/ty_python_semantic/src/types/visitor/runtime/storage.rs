//! Fixed collection operations surrounding the shared type walk's admitted payload growth.

use std::alloc::Layout;
use std::ptr::NonNull;
use std::slice::Iter;

use salsa::execution_probe::{RunError, RunResult};

use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, CALL_3, CHECKED_MULTIPLY, CHECK_LANGUAGE_UB,
    LAYOUT_FROM_SIZE_ALIGNMENT, MAYBE_IS_ALIGNED_AND_NOT_NULL, NONNULL_AS_PTR,
    NONNULL_NEW_UNCHECKED_WORK, OVERFLOWING_ADD, POINTER_ADD_WORK,
    POINTER_READ, RAW_POINTER_ADDRESS, RAW_SLICE_POINTER, SMALL_RETIRE_WRAPPERS, SMALL_TRIPLE_MUT,
    add_quotes, checked, event_quote,
    fixed_quote, prepared_smallvec_push_quote, smallvec_borrowed_quote, smallvec_new_quote,
    smallvec_metadata_quote, smallvec_pop_quote, smallvec_reserve_exact_quote,
};
use crate::types::constraints::control::{GrowthPlan, TddError, hash_slots, sequence_growth};
use crate::types::local_transfer::hash_table;
use crate::types::visitor::{SmallSet, SmallSetLayout, StoredTypeSequence, TypeCollector, TypeWalkCursor, TypeWalkFacts, WalkAction};
use crate::types::Type;

/// Quotes the search's empty pending stack and seen set, including their header retirement.
/// Later reservations pay for backing storage; inserted receiver cursors pay for their seen tables.
/// Constructing the `(TypeWalkCursor, TypeCollector)` result pair is included; its transfers
/// are paid by [`crate::types::local_transfer::local_quoted_with_fixed_transfers_at`].
pub(in crate::types) const fn search_initial_state_quote() -> RunResult<(usize, usize)> {
    checked(const {
        let pending = add_quotes(
            smallvec_new_quote::<WalkAction<'_>, 8>(),
            smallvec_borrowed_quote::<WalkAction<'_>>(SMALL_RETIRE_WRAPPERS),
        );
        let seen = add_quotes(
            smallvec_new_quote::<Type<'_>, 8>(),
            smallvec_borrowed_quote::<Type<'_>>(SMALL_RETIRE_WRAPPERS),
        );
        // The cursor only wraps its pending vector. The seen value additionally traverses
        // TypeCollector/RefCell/SmallSet defaults and the RefCell/Cell/UnsafeCell constructors.
        // Keep those owned carriers separate from the larger pending cursor. Six unary calls,
        // three zero-argument calls and sixteen field/aggregate/scalar events cover the pinned standard
        // library without debug_refcell's additional diagnostic Cell. Receiver arguments use
        // six reference slots; the borrow counter and its Cell wrappers use fourteen scalar slots.
        let wrappers = fixed_quote(6 * CALL_1 + 3 * 2 + 16, [
            (3, size_of::<TypeWalkCursor<'_>>()),
            (7, size_of::<TypeCollector<'_>>()),
            (13, size_of::<SmallSet<Type<'_>, 8>>()),
            (6, size_of::<&TypeWalkFacts>()),
            (14, size_of::<usize>()),
        ]);
        let containers = add_quotes(add_quotes(pending, seen), wrappers);
        add_quotes(containers, fixed_quote(1, [
            (1, size_of::<(TypeWalkCursor<'_>, TypeCollector<'_>)>()),
        ]))
    })
}

/// Quotes reading the pending stack's length and capacity and selecting its insertion quotation.
pub(super) const fn pending_metadata_quote() -> RunResult<(usize, usize)> {
    let metadata = match smallvec_metadata_quote::<WalkAction<'_>, 8>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    checked(add_quotes(metadata, event_quote(CALL_1 + 8, &[
        size_of::<usize>(), size_of::<bool>(), size_of::<RunResult<(usize, usize)>>(),
    ])))
}

/// Quotes one pending-frame removal and transfer, including an empty stack.
pub(super) const fn pending_pop_quote() -> RunResult<(usize, usize)> {
    smallvec_pop_quote::<WalkAction<'_>, 8>()
}

/// Quotes pending insertion after metadata is known, excluding the `TddWork::Grow` payload quote.
/// The action's header drop is paid at insertion; `OwnedConstraintTypeCursor` pays nested receiver storage.
pub(super) const fn pending_insert_quote(grew: bool, was_spilled: bool) -> RunResult<(usize, usize)> {
    let push = match prepared_smallvec_push_quote::<WalkAction<'_>, 8>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    let metadata = match smallvec_metadata_quote::<WalkAction<'_>, 8>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    // reserve_smallvec only calls sequence_growth and reserve_exact when capacity is exhausted.
    // Grow separately funds allocated payload and relocation units on that branch.
    let reservation = if grew {
        let reserve = match smallvec_reserve_exact_quote::<WalkAction<'_>, 8>(true, was_spilled) {
            Ok(quote) => Some(quote), Err(error) => return Err(error),
        };
        add_quotes(reserve, event_quote(6 * CALL_2 + 4 * CALL_1 + 28, &[
            size_of::<usize>(), size_of::<Option<usize>>(), size_of::<GrowthPlan>(),
        ]))
    } else {
        event_quote(CALL_1 + 2 * CALL_2 + OVERFLOWING_ADD + 8, &[
            size_of::<usize>(), size_of::<Option<usize>>(), size_of::<bool>(),
        ])
    };
    checked(add_quotes(add_quotes(push, metadata), add_quotes(reservation,
        fixed_quote(4, [(4, size_of::<WalkAction<'_>>())]),
    )))
}

/// Quotes advancing a slice-backed stored-type iterator and returning its copied type.
/// Ordered sets and multiple negative elements use indexmap's slice iterator; the empty/single
/// negative representation uses Option::take. Tree-backed typed-dictionary fields are excluded.
pub(super) const fn stored_slice_next_quote() -> RunResult<(usize, usize)> {
    checked(const {
        let slice_next = CALL_1 + CALL_2 + POINTER_ADD_WORK
            + 2 * NONNULL_NEW_UNCHECKED_WORK + 2 * NONNULL_AS_PTR + 22;
        let adapters = 6 * CALL_1 + 3 * CALL_2 + 18;
        add_quotes(event_quote(slice_next + adapters, &[
            size_of::<&mut StoredTypeSequence<'_>>(), size_of::<Option<&Type<'_>>>(),
            size_of::<Option<Type<'_>>>(), size_of::<&Type<'_>>(),
            size_of::<usize>(), size_of::<bool>(),
        ]), fixed_quote(0, [(4, size_of::<Type<'_>>())]))
    })
}

/// Returns whether the seen set is inline, its length, and its capacity, without hashing
/// or comparing a type.
pub(super) fn seen_metadata(seen: &mut TypeCollector<'_>) -> (bool, usize, usize) {
    let layout = seen.0.get_mut().layout();
    (layout.inline, layout.len, layout.capacity)
}

/// Quotes the metadata read and selection of preparation costs before a seen-set insertion.
pub(super) const fn seen_metadata_quote() -> RunResult<(usize, usize)> {
    let metadata = match small_set_metadata_quote::<Type<'_>, 8>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    checked(add_quotes(metadata, event_quote(CALL_1 + 4, &[
        size_of::<SmallSetLayout>(), size_of::<(bool, usize, usize)>(),
    ])))
}

/// Quotes a small set's representation, length and capacity without accessing its keys.
pub(in crate::types) const fn small_set_metadata_quote<K, const N: usize>() -> RunResult<(usize, usize)> {
    let small = match smallvec_metadata_quote::<K, N>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    checked(add_quotes(small, event_quote(12 * CALL_1 + 32, &[
        size_of::<&SmallSet<K, N>>(), size_of::<SmallSetLayout>(),
        size_of::<usize>(), size_of::<bool>(), size_of::<RunResult<(usize, usize)>>(),
    ])))
}

/// Quotes seen-set scans, hash backing, and backing cleanup beyond the existing Grow payload.
/// Hash lookup work remains one logical access; an inline scan separately counts each comparison.
pub(super) fn seen_payload_quote(
    inline: bool,
    len: usize,
    capacity: usize,
    type_payload: usize,
) -> RunResult<(usize, usize)> {
    let overflow = || RunError::Contract("type walk seen quotation overflow");
    let access = type_payload.checked_add(1).ok_or_else(overflow)?;
    // insert_with already pays one candidate access. Inline equality may inspect every
    // retained element; spilled contains followed by insert needs one additional access.
    let additional_accesses = if inline { len.saturating_sub(1) } else { 1 };
    let access_work = access.checked_mul(additional_accesses).ok_or_else(overflow)?;
    let native = if inline {
        inline_scan_quote::<Type<'_>>(len)?
    } else {
        let access = const { hash_table::access_quote::<Type<'_>, ()>() }?;
        checked(add_quotes(Some(access), Some(access)))?
    };
    // Each admitted key access and growth now passes through a SmallSetControl adapter.
    let callbacks = if len < capacity { 1 } else { len.checked_add(2).ok_or_else(overflow)? };
    let adapter = event_quote(callbacks.checked_mul(CALL_2 + 8).ok_or_else(overflow)?, &[
        size_of::<Type<'_>>(), size_of::<GrowthPlan>(), size_of::<&mut ()>(),
        size_of::<Result<(), TddError<RunError>>>(),
    ]);
    let native = checked(add_quotes(Some(native), adapter))?;
    if len < capacity {
        let insertion = if inline {
            const { prepared_smallvec_push_quote::<Type<'_>, 8>() }?
        } else { (0, 0) };
        return checked(add_quotes(Some(native), add_quotes(Some(insertion), Some((access_work, 0)))));
    }
    let plan = if inline {
        GrowthPlan {
            requested_capacity: 9,
            requested_payload_bytes: 9 * size_of::<Type<'_>>(),
            relocation_units: 8,
        }
    } else {
        let required = len.checked_add(1).ok_or_else(overflow)?;
        sequence_growth::<Type<'_>, RunError>(capacity, required).map_err(|_| overflow())?
    };
    let maximum_capacity = plan.requested_capacity.checked_mul(2).ok_or_else(overflow)?;
    let new_slots = hash_slots::<RunError>(maximum_capacity).map_err(|_| overflow())?;
    let old_slots = if inline { 0 } else {
        hash_slots::<RunError>(capacity).map_err(|_| overflow())?
    };
    let bytes = new_slots.checked_mul(size_of::<Type<'_>>() + 1).ok_or_else(overflow)?;
    Layout::from_size_align(bytes, align_of::<Type<'_>>()).map_err(|_| overflow())?;
    let bytes = bytes.checked_sub(plan.requested_payload_bytes).ok_or_else(overflow)?;
    let work = new_slots.checked_add(old_slots)
        .and_then(|slots| slots.checked_mul(2))
        .and_then(|slots| slots.checked_add(access_work))
        .and_then(|work| work.checked_add(if inline { access } else { 0 }))
        .ok_or_else(overflow)?;
    let growth = hash_table::growth_quote::<Type<'_>, ()>(old_slots, new_slots, if inline { 0 } else { len })?;
    let scan = if inline {
        const { inline_spill_quote::<Type<'_>, 8>() }?
    } else {
        // insert_with explicitly iterates all old keys before reserve; the resize quote
        // separately accounts for hashbrown's own old-bucket scan.
        hash_table::scan_quote::<Type<'_>, ()>(old_slots)?
    };
    checked(add_quotes(add_quotes(Some(native), Some((work, bytes))),
        add_quotes(Some(growth), Some(scan))))
}

/// Quotes preparing the seen-set payload supplement for its current representation and capacity.
/// Checked growth, layout, and dynamic hash growth/scan quotations run only when the table is full.
pub(super) const fn seen_payload_preparation_quote(inline: bool, full: bool) -> RunResult<(usize, usize)> {
    if !full {
        // The generic scan helper and adapter supplement add checked callback scaling,
        // quote aggregation and their argument/result transfers to the existing preparation.
        let (multiplications, additions) = if inline { (5, 12) } else { (3, 10) };
        return checked(event_quote(
            multiplications * CHECKED_MULTIPLY + additions * OVERFLOWING_ADD
                + 43 * CALL_2 + 30 * CALL_1 + 124,
            &[
                size_of::<usize>(), size_of::<Option<usize>>(), size_of::<bool>(),
                size_of::<RunResult<(usize, usize)>>(),
            ],
        ));
    }
    let hash = match hash_table::preparation_quote() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    checked(add_quotes(add_quotes(hash, hash), const { event_quote(
        10 * CHECKED_MULTIPLY + 15 * OVERFLOWING_ADD + LAYOUT_FROM_SIZE_ALIGNMENT
            + 43 * CALL_2 + 30 * CALL_1 + 124,
        &[
            size_of::<usize>(), size_of::<Option<usize>>(), size_of::<bool>(),
            size_of::<GrowthPlan>(), size_of::<Layout>(),
            size_of::<RunResult<(usize, usize)>>(),
            size_of::<Result<Layout, std::alloc::LayoutError>>(),
        ],
    ) }))
}

const INLINE_SLICE: usize = 3 * CALL_1 + SMALL_TRIPLE_MUT + 2 * CALL_1 + 8
    + CALL_2 + CHECK_LANGUAGE_UB + CALL_3 + MAYBE_IS_ALIGNED_AND_NOT_NULL
    + CHECKED_MULTIPLY + RAW_SLICE_POINTER + 16;

/// Quotes constructing the borrowed iterator used by membership and retained-key scans.
const fn inline_scan_start_quote<K>() -> RunResult<(usize, usize)> {
    checked(const { smallvec_borrowed_quote::<K>(
        INLINE_SLICE + 5 * CALL_1 + NONNULL_NEW_UNCHECKED_WORK + POINTER_ADD_WORK + 16,
    ) })
}

// Iter::next advances non-ZST keys through NonNull::add's offset intrinsic. For ZST keys,
// the end pointer instead encodes the remaining count: next reads that address, subtracts
// one with a checked precondition, and reconstructs the count pointer without provenance.
// Neither path invokes NonNull::new_unchecked or raw-pointer add. Keep the scan's
// call/dispatch envelope separate from both paths.
const INLINE_SCAN_WRAPPERS: usize = 4 * CALL_1 + 2 * CALL_2 + 24;
const INLINE_POINTER_EQUAL: usize = CALL_2 + 2 * NONNULL_AS_PTR + 4;
const INLINE_POINTER_ADVANCE: usize = CALL_2 + NONNULL_AS_PTR + CALL_2 + CALL_1 + 8;
const INLINE_POINTER_REFERENCE: usize = CALL_1 + NONNULL_AS_PTR + CALL_1 + 4;
// unchecked_sub includes the successful language-UB check, precondition callback,
// overflowing_sub/sub_with_overflow and final unchecked subtraction. Provenance
// reconstruction uses a unary wrapper and transmute; both paths then borrow the key.
const INLINE_LENGTH_ADVANCE: usize = RAW_POINTER_ADDRESS + 5 * CALL_2 + CHECK_LANGUAGE_UB + 16
    + 2 * CALL_1 + 4;

/// Quotes one yielded key and its scan wrapper, excluding key equality or admission callbacks.
/// One bound covers both non-zero-sized and zero-sized keys.
const fn inline_scan_step_quote<K>() -> RunResult<(usize, usize)> {
    let advance = if INLINE_LENGTH_ADVANCE > INLINE_POINTER_EQUAL + INLINE_POINTER_ADVANCE {
        INLINE_LENGTH_ADVANCE
    } else {
        INLINE_POINTER_EQUAL + INLINE_POINTER_ADVANCE
    };
    checked(event_quote(
        INLINE_SCAN_WRAPPERS + advance + INLINE_POINTER_REFERENCE,
        &[size_of::<Iter<'_, K>>(), size_of::<(NonNull<K>, usize)>(),
            size_of::<(&K, &K)>(), size_of::<Option<&K>>(), size_of::<(usize, bool)>(),
            size_of::<usize>(), size_of::<bool>()],
    ))
}

/// Quotes the terminal iterator check, which neither advances nor produces a key reference.
/// One bound covers both pointer endpoints and the zero-sized-key length counter.
const fn inline_scan_terminal_quote<K>() -> RunResult<(usize, usize)> {
    let empty_check = if RAW_POINTER_ADDRESS > INLINE_POINTER_EQUAL {
        RAW_POINTER_ADDRESS
    } else {
        INLINE_POINTER_EQUAL
    };
    checked(event_quote(
        INLINE_SCAN_WRAPPERS + empty_check,
        &[size_of::<Iter<'_, K>>(), size_of::<(NonNull<K>, NonNull<K>)>(),
            size_of::<Option<&K>>(),
            size_of::<usize>(), size_of::<bool>()],
    ))
}

/// Quotes iterator construction and its terminal check before any dynamic scan scaling.
const fn inline_scan_fixed_quote<K>() -> RunResult<(usize, usize)> {
    let start = match inline_scan_start_quote::<K>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    let terminal = match inline_scan_terminal_quote::<K>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    checked(add_quotes(start, terminal))
}

/// Quotes the inline membership scan, including its terminal step but excluding key equality.
pub(in crate::types) fn inline_scan_quote<K>(len: usize) -> RunResult<(usize, usize)> {
    let overflow = || RunError::Contract("small set inline scan quotation overflow");
    let (work, bytes) = const { inline_scan_step_quote::<K>() }?;
    let repeated = work.checked_mul(len).zip(bytes.checked_mul(len)).ok_or_else(overflow)?;
    let fixed = const { inline_scan_fixed_quote::<K>() }?;
    checked(add_quotes(Some(fixed), Some(repeated)))
}

/// Quotes draining a full inline buffer and inserting its keys and the new candidate into a table.
/// Table allocation/retirement and key Hash/Eq bodies are paid separately.
/// The preceding retained-key scan is included, excluding its admission callback bodies.
pub(in crate::types) const fn inline_spill_quote<K: Copy, const N: usize>() -> RunResult<(usize, usize)> {
    let access = match hash_table::access_quote::<K, ()>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    // drain(..) empties the whole inline buffer: no tail is moved. Each
    // yielded value uses slice next/map/read before HashSet::extend inserts it.
    let Some(entries_and_terminal) = N.checked_add(1) else {
        return Err(RunError::Contract("small set spill length overflow"));
    };
    let Some(drain_steps) = entries_and_terminal.checked_mul(5 * CALL_1 + 2 * CALL_2 + POINTER_READ + POINTER_ADD_WORK + 24) else {
        return Err(RunError::Contract("small set spill work overflow"));
    };
    let Some(transfers) = N.checked_mul(4) else {
        return Err(RunError::Contract("small set spill transfers overflow"));
    };
    let drain = smallvec_borrowed_quote::<K>(
        8 * CALL_1 + 3 * CALL_2 + INLINE_SLICE + POINTER_ADD_WORK
            + NONNULL_NEW_UNCHECKED_WORK + 32
            + drain_steps,
    );
    let mut entries = None;
    let mut scan = match inline_scan_fixed_quote::<K>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    let scan_step = match inline_scan_step_quote::<K>() {
        Ok(quote) => Some(quote), Err(error) => return Err(error),
    };
    let mut index = 0;
    while index < entries_and_terminal {
        entries = if index == 0 { access } else { add_quotes(entries, access) };
        if index < N {
            scan = add_quotes(scan, scan_step);
        }
        index += 1;
    }
    checked(add_quotes(add_quotes(add_quotes(drain, entries), scan), fixed_quote(8, [
        (transfers, size_of::<K>()), (4, size_of::<smallvec::SmallVec<[K; N]>>()),
    ])))
}
