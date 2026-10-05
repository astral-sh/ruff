//! Native wrappers for insert-only, Global-allocated hash tables with Copy entries.
//!
//! A lookup or insertion remains one logical hash access. Explicit scans, control-byte
//! initialization and entry relocation are separate progress. These quotations describe
//! the pinned hashbrown 0.17.1 aarch64 NEON path; key Hash/Eq bodies and backing payload
//! allocation are supplied by the caller.

use std::alloc::Layout;
use std::ptr::NonNull;

use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use salsa::execution_probe::{RunError, RunResult};

use super::collections::{
    ALIGNMENT_NEW, CALL_0, CALL_1, CALL_2, CALL_3, CALL_4, CALL_5, CHECK_LANGUAGE_UB,
    CHECKED_MULTIPLY, COPY_PRECONDITION, GLOBAL_ALLOCATION, GLOBAL_DEALLOCATION,
    IS_POWER_OF_TWO, LAYOUT_SIZE_VALID, MAYBE_IS_ALIGNED_AND_NOT_NULL, NONNULL_AS_PTR,
    NONNULL_CAST, NONNULL_NEW_UNCHECKED_WORK, POINTER_ADD_WORK, POINTER_PRECONDITION,
    POINTER_READ, POINTER_WRITE, RAW_POINTER_ADDRESS, RAW_SLICE_POINTER, FixedQuote,
    QUOTATION_AND_THEN as AND_THEN, QUOTATION_CHECKED_ARITHMETIC as ARITHMETIC,
    QUOTATION_OK_OR as OK_OR, QUOTATION_RESULT_TRY as PROPAGATE, QUOTATION_ZIP as ZIP,
    add_quotes, checked, event_quote,
};

const CALL_6: usize = 18 + 2;
const CALL_7: usize = 21 + 2;
const CALL_8: usize = 24 + 2;

// Pointer and NEON wrappers reach the checked read/intrinsic leaves. Tag has stride one,
// so its align_offset path cannot enter the modular-inverse loop.
const SUB: usize = POINTER_ADD_WORK + 2 * CALL_2 + 16;
const ALIGN_TAG: usize = 2 * CALL_2 + IS_POWER_OF_TWO + RAW_POINTER_ADDRESS + CALL_0
    + 7 * CALL_2 + CALL_1 + 64;
const READ_UNALIGNED8: usize = 4 * CALL_1 + POINTER_READ + 8;
const REINTERPRET: usize = 2 * CALL_1 + 2;
const LANE: usize = CALL_1 + CALL_2 + 2;
const CMP_ZERO: usize = CALL_1 + CALL_8 + CALL_1 + CALL_2 + 16;
const GROUP_LOAD: usize = 4 * CALL_1 + READ_UNALIGNED8 + 4;
const GROUP_LOAD_ALIGNED: usize = GROUP_LOAD + ALIGN_TAG + 2 * CALL_0 + CALL_2 + 16;
const GROUP_MATCH_SPECIAL: usize = CALL_1 + 2 * REINTERPRET + CMP_ZERO + LANE + 16;
const TRY: usize = 4 * CALL_1 + 16;
const UNWRAP_PRESENT: usize = CALL_1 + 8;
const NONZERO_NEW: usize = 2 * CALL_1 + 2;
const NONZERO_GET: usize = 2 * CALL_1 + 2;
const NONZERO_TRAILING: usize = CALL_1 + NONZERO_GET + CALL_1 + 4;
const LOWEST_BIT: usize = CALL_1 + NONZERO_NEW + CALL_1 + NONZERO_TRAILING + 16;
const REMOVE_BIT: usize = CALL_1 + 8;
const BIT_NEXT: usize = CALL_1 + LOWEST_BIT + REMOVE_BIT + TRY + 16;
const BIT_INTO: usize = CALL_1 + 8;
const DATA_END: usize = CALL_1 + NONNULL_CAST + 8;
const FROM_BASE: usize = CALL_2 + NONNULL_AS_PTR + SUB + NONNULL_NEW_UNCHECKED_WORK + 16;
const BUCKET: usize = CALL_2 + DATA_END + FROM_BASE + 2 * CALL_1 + 32;
const BUCKET_AS_PTR: usize = CALL_1 + NONNULL_AS_PTR + SUB + 8;
const BUCKET_REF: usize = CALL_1 + BUCKET_AS_PTR + 4;
const BUCKET_WRITE: usize = CALL_2 + BUCKET_AS_PTR + CALL_2 + POINTER_WRITE + 8;
const BUCKET_NEXT: usize = CALL_2 + NONNULL_AS_PTR + SUB + NONNULL_NEW_UNCHECKED_WORK + 16;
const BUCKET_PTR: usize = CALL_3 + DATA_END + NONNULL_AS_PTR + SUB + 2 * CALL_1 + 32;
const CTRL: usize = CALL_2 + NONNULL_AS_PTR + POINTER_ADD_WORK + 2 * CALL_1 + 24;
const TAG: usize = CALL_1 + 16;
const PROBE_START: usize = CALL_2 + CALL_1 + 16;

// The capacity-dependent probe loop is within the logical hash-access boundary.
// These fixed wrappers include std/set forwarding, make_hash and prepared insertion.
const HASH_WRAPPER: usize = 6 * CALL_2 + 4 * CALL_1 + 2 * CALL_0 + 48;
const FIX_INSERT: usize = 2 * CALL_2 + 2 * CTRL + GROUP_LOAD_ALIGNED
    + GROUP_MATCH_SPECIAL + LOWEST_BIT + UNWRAP_PRESENT + 3 * CALL_1 + 64;
const SET_CTRL: usize = CALL_3 + 2 * CALL_2 + 2 * CTRL + 32;
const RECORD_INSERT: usize = CALL_4 + 3 * CALL_1 + SET_CTRL + 32;
const INSERT_WRITE: usize = 2 * CALL_4 + TAG + CTRL + RECORD_INSERT + BUCKET + BUCKET_WRITE + 48;
const LOOKUP_FIXED: usize = 4 * CALL_2 + 3 * CALL_3 + 5 * CALL_1 + TAG + PROBE_START
    + BUCKET + BUCKET_REF + HASH_WRAPPER + 128;
const INSERT_FIXED: usize = 4 * CALL_3 + 4 * CALL_2 + 4 * CALL_1 + CALL_4 + TAG + PROBE_START
    + HASH_WRAPPER + FIX_INSERT + INSERT_WRITE + 128;
const ACCESS_FIXED: usize = LOOKUP_FIXED + INSERT_FIXED;
const FAST_RESERVE: usize = 3 * CALL_2 + CALL_3 + 3 * CALL_1 + 32;
const EMPTY_TOTAL: usize = 4 * CALL_0 + 8 * CALL_1 + CALL_3 + 96;

const SCAN_FIXED: usize = 12 * CALL_1 + CALL_3 + 2 * POINTER_ADD_WORK + DATA_END + FROM_BASE
    + CTRL + GROUP_LOAD_ALIGNED + GROUP_MATCH_SPECIAL + BIT_INTO
    + NONNULL_NEW_UNCHECKED_WORK + 128;
const SCAN_SLOT: usize = 12 * CALL_1 + 2 * CALL_2 + 2 * BIT_NEXT + 2 * BUCKET_NEXT + BUCKET_REF
    + GROUP_LOAD_ALIGNED + GROUP_MATCH_SPECIAL + BIT_INTO
    + POINTER_ADD_WORK + NONNULL_AS_PTR + NONNULL_NEW_UNCHECKED_WORK + 128;
// FullBucketsIndices::next returns None when items is zero, before next_impl.
// Keep its call, items read, zero comparison, branch and Option/return transfers.
const EMPTY_SCAN_TERMINAL: usize = CALL_1 + 8;
const CHECKED_ADD: usize = 3 * CALL_2 + CALL_1 + 16;
const NEXT_POWER: usize = 3 * CALL_1 + 24;
const MAX_USIZE: usize = 3 * CALL_2 + 16;
const CAPACITY_FROM_MASK: usize = CALL_1 + 24;
const CAPACITY_TO_BUCKETS: usize = CALL_2 + 2 * CHECKED_MULTIPLY + NEXT_POWER
    + MAX_USIZE + 2 * CALL_2 + 2 * TRY + 128;
const LAYOUT_UNCHECKED: usize = 3 * CALL_2 + CHECK_LANGUAGE_UB + ALIGNMENT_NEW
    + IS_POWER_OF_TWO + LAYOUT_SIZE_VALID + 2 * CALL_1 + 64;
const CALCULATE_LAYOUT: usize = CALL_2 + IS_POWER_OF_TWO + CHECKED_MULTIPLY
    + 2 * CHECKED_ADD + 3 * TRY + LAYOUT_UNCHECKED + 128;
const SLICE_MUT: usize = CALL_2 + CHECK_LANGUAGE_UB + CALL_4
    + MAYBE_IS_ALIGNED_AND_NOT_NULL + CALL_2 + RAW_SLICE_POINTER + 48;
const CTRL_SLICE: usize = CALL_1 + NONNULL_AS_PTR + 2 * CALL_1 + SLICE_MUT + 16;
const FILL_CONTROLS: usize = CTRL_SLICE + 4 * CALL_1 + CALL_2 + 3 * CALL_3 + POINTER_PRECONDITION + 48;
const ALLOCATION_INFO: usize = CALL_2 + CALCULATE_LAYOUT + NONNULL_AS_PTR + SUB
    + NONNULL_NEW_UNCHECKED_WORK + UNWRAP_PRESENT + 2 * CALL_1 + 48;
const FREE_BUCKETS: usize = CALL_3 + ALLOCATION_INFO + GLOBAL_DEALLOCATION + 16;
const RETIRE_FIXED: usize = 4 * CALL_1 + CALL_3 + FREE_BUCKETS + 64;
const NEW_TABLE: usize = CALL_4 + CAPACITY_TO_BUCKETS + CALL_4 + CALCULATE_LAYOUT
    + CALL_2 + GLOBAL_ALLOCATION + NONNULL_CAST + NONNULL_AS_PTR
    + POINTER_ADD_WORK + NONNULL_NEW_UNCHECKED_WORK
    + CAPACITY_FROM_MASK + FILL_CONTROLS + 8 * CALL_1 + 4 * TRY + 160;
const RESIZE_FIXED: usize = 3 * CALL_2 + CALL_1 + CALL_3 + CALL_4 + CALL_7 + CALL_6 + CALL_5
    + CHECKED_ADD + CAPACITY_FROM_MASK + MAX_USIZE + NEW_TABLE
    + 2 * CALL_2 + 8 * CALL_1 + 4 * TRY + 256 + 2 * RETIRE_FIXED;
const DESTINATION_PROBE_FIXED: usize = CALL_2 + PROBE_START + FIX_INSERT + CALL_1 + 24;
const COPY_ENTRY: usize = 2 * BUCKET_PTR + 2 * CALL_3 + COPY_PRECONDITION + 2 * CALL_1 + 32;
const REHASH_ENTRY: usize = CALL_3 + HASH_WRAPPER + BUCKET + BUCKET_REF
    + DESTINATION_PROBE_FIXED + TAG + CTRL + SET_CTRL + COPY_ENTRY + 8 * CALL_1 + 64;

const fn native_quote<K, V>(work: usize) -> FixedQuote {
    event_quote(work, &[
        // reserve_rehash_inner retains a fat hasher reference and two-word TableLayout.
        size_of::<(*mut (), *const (), usize, &dyn Fn(*mut (), usize) -> u64,
            usize, (usize, usize), Option<unsafe fn(*mut u8)>)>(),
        size_of::<(*mut (), u64, usize, (K, V))>(), size_of::<(*mut (), K, V)>(),
        size_of::<(*mut (K, V), (K, V))>(), size_of::<Option<V>>(), size_of::<(K, V)>(),
        size_of::<FxHashMap<K, V>>(), size_of::<FxHashSet<K>>(), size_of::<FxBuildHasher>(),
        // The resize guard owns the four-word table and its allocator/layout capture.
        size_of::<([usize; 4], (*const (), (usize, usize)), usize)>(),
        size_of::<[usize; 5]>(), size_of::<(*const (), *mut (), usize, usize, usize)>(),
        size_of::<(*const (), Layout, bool)>(), size_of::<(*const (), NonNull<u8>, Layout)>(),
        size_of::<Option<(Layout, usize)>>(), size_of::<(NonNull<u8>, Layout)>(),
        size_of::<Option<(&K, &V)>>(),
    ])
}

fn scaled(quote: RunResult<(usize, usize)>, count: usize) -> RunResult<(usize, usize)> {
    let (work, bytes) = quote?;
    work.checked_mul(count).zip(bytes.checked_mul(count))
        .ok_or(RunError::Contract("hash table quotation overflow"))
}

/// Quotes an empty map/set header and its empty retirement, without allocation.
pub(in crate::types) const fn empty_quote<K: Copy, V: Copy>() -> RunResult<(usize, usize)> {
    checked(const { native_quote::<K, V>(EMPTY_TOTAL) })
}

/// Quotes fixed lookup or prepared-insert wrappers, including insertion's reserve fast path.
/// The logical hash-access unit, key Hash/Eq bodies, growth and scans remain with the caller.
pub(in crate::types) const fn access_quote<K: Copy, V: Copy>() -> RunResult<(usize, usize)> {
    checked(const { native_quote::<K, V>(ACCESS_FIXED) })
}

/// Quotes fixed native lookup wrappers and their borrowed argument/result carriers.
/// The caller separately funds the logical access and key Hash/Eq bodies.
pub(in crate::types) const fn lookup_quote<K: Copy, V: Copy>() -> RunResult<(usize, usize)> {
    checked(const { event_quote(LOOKUP_FIXED, &[
        // RawTableInner::find_inner receives the complete fat equality callback.
        size_of::<(*const (), u64, &mut dyn FnMut(usize) -> bool)>(),
        size_of::<(*const (), u64, *const ())>(), size_of::<(&K, &K)>(),
        size_of::<Option<&V>>(), size_of::<Option<&(K, V)>>(),
        size_of::<Option<(&K, &V)>>(), size_of::<Option<usize>>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<u64>(), size_of::<[u8; 16]>(),
    ]) })
}

/// Quotes fixed insertion wrappers after capacity is reserved, including the internal fast reserve.
/// Complete owned-entry carriers remain sized for K and V; key Hash/Eq and logical access are separate.
pub(in crate::types) const fn prepared_insert_quote<K: Copy, V: Copy>() -> RunResult<(usize, usize)> {
    checked(const { event_quote(INSERT_FIXED, &[
        // find_or_find_insert_index receives sized equality and hasher closures.
        // Its inner call borrows the index-equality callback as a trait object.
        size_of::<(*mut (), u64, *const (), *const ())>(),
        size_of::<(*mut (), u64, &mut dyn FnMut(usize) -> bool)>(),
        size_of::<(*mut (), u64, usize, (K, V))>(), size_of::<(*mut (), K, V)>(),
        size_of::<(*mut (K, V), (K, V))>(), size_of::<Option<V>>(), size_of::<(K, V)>(),
        size_of::<Result<NonNull<(K, V)>, usize>>(), size_of::<Option<usize>>(),
        size_of::<(*mut (), usize, *const ())>(), size_of::<(&K, &K)>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<u64>(), size_of::<[u8; 16]>(),
    ]) })
}

/// Quotes an explicit reserve whose required capacity is already available.
pub(in crate::types) const fn reserve_quote<K: Copy, V: Copy>() -> RunResult<(usize, usize)> {
    checked(const { native_quote::<K, V>(FAST_RESERVE) })
}

/// Quotes a complete borrowed-key or old-bucket scan, including its terminating step.
/// `slots` bounds buckets plus one control group; callback bodies are separate.
pub(in crate::types) fn scan_quote<K: Copy, V: Copy>(slots: usize) -> RunResult<(usize, usize)> {
    let steps = slots.checked_add(1).ok_or(RunError::Contract("hash table scan overflow"))?;
    let repeated = scaled(const { checked(native_quote::<K, V>(SCAN_SLOT)) }, steps)?;
    checked(add_quotes(const { native_quote::<K, V>(SCAN_FIXED) }, Some(repeated)))
}

/// Quotes allocating an insert-only replacement, scanning/copying entries and both retirements.
/// Slot bounds include the control group. The caller pays backing allocation and key hashing;
/// this quote includes control-byte writes and copied entry bytes. Each rehashed entry needs
/// one separately admitted logical hash access, regardless of replacement capacity.
pub(in crate::types) fn growth_quote<K: Copy, V: Copy>(
    old_slots: usize,
    new_slots: usize,
    len: usize,
) -> RunResult<(usize, usize)> {
    let scan = if len == 0 {
        // Construction still loads the initial control group for an empty table.
        const { checked(native_quote::<K, V>(SCAN_FIXED + EMPTY_SCAN_TERMINAL)) }?
    } else {
        // FullBucketsIndices stops after len live entries. Before the last entry it may
        // cross empty groups, so also bound those advances using the table's slot bound.
        // The first eight-byte NEON group is loaded by SCAN_FIXED; terminal next only
        // checks the exhausted item count, without scanning any trailing empty groups.
        let group_advances = old_slots.div_ceil(8).saturating_sub(1);
        let steps = len.checked_add(group_advances)
            .ok_or(RunError::Contract("hash table resize scan overflow"))?;
        let repeated = scaled(const { checked(native_quote::<K, V>(SCAN_SLOT)) }, steps)?;
        checked(add_quotes(
            const { native_quote::<K, V>(SCAN_FIXED + EMPTY_SCAN_TERMINAL) },
            Some(repeated),
        ))?
    };
    let entries = scaled(const { checked(native_quote::<K, V>(REHASH_ENTRY)) }, len)?;
    let payload = len.checked_mul(size_of::<(K, V)>()).and_then(|bytes| bytes.checked_mul(2))
        .and_then(|bytes| bytes.checked_add(new_slots))
        .ok_or(RunError::Contract("hash table relocation bytes overflow"))?;
    checked(add_quotes(add_quotes(const { native_quote::<K, V>(RESIZE_FIXED) }, Some(scan)),
        add_quotes(Some(entries), Some((new_slots, payload)))))
}

/// Quotes evaluating one dynamic scan/growth quotation before reading its scalar inputs.
pub(in crate::types) const fn preparation_quote() -> RunResult<(usize, usize)> {
    // Calls include their complete argument groups. Each four-event scalar group bounds
    // reads/bindings, dispatch, construction and forwarding at one named source stage.
    // Checked arithmetic includes overflowing_mul and its intrinsic, or both add
    // intrinsics, plus unlikely and its overflow-only cold_path call. The seven scalar
    // groups cover casts, the intrinsic result, two tuple bindings, tuple reconstruction,
    // unlikely's branch and the final Option branch.
    // Result propagation includes branch, from_residual and identity From. Its scalar
    // groups cover result dispatch, residual construction, caller dispatch and forwarding.
    const CHECKED: usize = CALL_1 + 3 * 4;
    // add_quotes has two checked additions, three groups for its input destructuring,
    // two for the checked-result branches and one for the returned pair and Some.
    const ADD: usize = CALL_2 + 2 * ARITHMETIC + 6 * 4;
    // scaled also extracts its input pair, constructs an eager error and forwards the result.
    const SCALED: usize = CALL_2 + PROPAGATE + 2 * ARITHMETIC + ZIP + OK_OR + 3 * 4;
    const SCAN: usize = CALL_1 + ARITHMETIC + OK_OR + 2 * PROPAGATE + SCALED
        + ADD + CHECKED + 7 * 4;
    // Resize scans also divide the slot bound into control groups, subtract the loaded
    // group and combine the remaining advances with the live-entry count.
    // div_ceil includes division/remainder checks and the final addition check; its
    // divisor is eight, so these cannot panic. saturating_sub calls its native intrinsic.
    const DIVIDE: usize = CALL_2 + 8 * 4;
    const SATURATING_SUBTRACT: usize = 2 * CALL_2 + 4;
    // The nonempty path bounds the empty branch. Besides these composed helpers, its
    // 23 scalar groups cover dispatch/bindings (seven), constants/variants/pairs (nine),
    // eager errors (two), closure construction/capture access (three), and forwarding (two).
    // size_of remains a runtime zero-argument call in growth_quote's payload expression.
    const GROWTH: usize = CALL_3 + DIVIDE + SATURATING_SUBTRACT + ARITHMETIC
        + 2 * SCALED + 4 * ADD + 2 * CHECKED + 3 * ARITHMETIC + 2 * AND_THEN
        + 2 * OK_OR + 5 * PROPAGATE + CALL_0 + 4 + 23 * 4;
    const WORK: usize = if SCAN > GROWTH { SCAN } else { GROWTH };
    checked(const { event_quote(WORK, &[
        size_of::<(usize, usize, usize)>(), size_of::<(usize, bool)>(),
        size_of::<(FixedQuote, FixedQuote)>(), size_of::<(FixedQuote, RunError)>(),
        size_of::<(RunResult<(usize, usize)>, usize)>(),
        size_of::<(Option<usize>, Option<usize>)>(),
        size_of::<(Option<usize>, RunError)>(), size_of::<(Option<usize>, &usize)>(),
        size_of::<(&usize, (usize,))>(),
        size_of::<RunResult<(usize, usize)>>(), size_of::<RunResult<usize>>(),
        size_of::<RunResult<std::convert::Infallible>>(),
        size_of::<std::ops::ControlFlow<RunResult<std::convert::Infallible>, (usize, usize)>>(),
        size_of::<std::ops::ControlFlow<RunResult<std::convert::Infallible>, usize>>(),
    ]) })
}
