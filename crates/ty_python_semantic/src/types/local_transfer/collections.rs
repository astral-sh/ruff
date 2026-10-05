//! Shared fixed quotations for collection construction, transfers and retirement.

use std::alloc::Layout;
use std::collections::TryReserveError;
use std::ptr::NonNull;

use salsa::execution_probe::{RunError, RunResult};

// Calls fund an expression read, argument initialization and parameter binding per argument,
// followed by call and return. Representation widths contribute only to the separate byte quote.
pub(in crate::types) const CALL_0: usize = 2;
pub(in crate::types) const CALL_1: usize = 3 + 2;
pub(in crate::types) const CALL_2: usize = 6 + 2;
pub(in crate::types) const CALL_3: usize = 9 + 2;
pub(in crate::types) const CALL_4: usize = 12 + 2;
pub(in crate::types) const CALL_5: usize = 15 + 2;

// Dynamic quotation preparation includes the native checked-arithmetic bodies and
// overflow paths. Each scalar group bounds at most four reads, bindings, constructions
// or dispatches: casts, intrinsic result, two tuple bindings, tuple reconstruction,
// unlikely's branch and the final Option branch. Calls include cold_path on overflow.
pub(in crate::types) const QUOTATION_CHECKED_ARITHMETIC: usize =
    3 * CALL_2 + CALL_1 + CALL_0 + 7 * 4;
// Result propagation includes branch, from_residual and identity From. Scalar groups
// cover Result dispatch, ControlFlow construction, caller dispatch and residual return.
pub(in crate::types) const QUOTATION_RESULT_TRY: usize = 3 * CALL_1 + 4 * 4;
// These Option bodies include their calls, dispatch, construction and forwarding.
// Callers separately fund eager error construction and closure creation/body work.
pub(in crate::types) const QUOTATION_OK_OR: usize = CALL_2 + 3 * 4;
pub(in crate::types) const QUOTATION_ZIP: usize = CALL_2 + 4 * 4;
pub(in crate::types) const QUOTATION_AND_THEN: usize = 2 * CALL_2 + 3 * 4;

// const_eval_select packages its captures and selects a function item before the runtime call.
pub(in crate::types) const SELECT_0: usize = CALL_3 + 2 + 5;
pub(in crate::types) const SELECT_1: usize = CALL_3 + CALL_1 + 6;
pub(in crate::types) const SELECT_2: usize = CALL_3 + CALL_2 + 7;
pub(in crate::types) const SELECT_3: usize = CALL_3 + CALL_3 + 8;
pub(in crate::types) const CHECK_LANGUAGE_UB: usize = 2 + SELECT_0 + 3 + 2 + 1 + 3;
pub(in crate::types) const RAW_POINTER_ADDRESS: usize = 3 * CALL_1 + 2;
pub(in crate::types) const CONST_POINTER_IS_NULL: usize = CALL_1 + 3 + SELECT_1 + RAW_POINTER_ADDRESS + 3;
pub(in crate::types) const MUT_POINTER_IS_NULL: usize = 2 * CALL_1 + 1 + CONST_POINTER_IS_NULL;
pub(in crate::types) const IS_POWER_OF_TWO: usize = 3 * CALL_1 + 4;
pub(in crate::types) const IS_ALIGNED_TO: usize = CALL_2 + IS_POWER_OF_TWO + RAW_POINTER_ADDRESS + 13;
pub(in crate::types) const MAYBE_IS_ALIGNED: usize = CALL_2 + SELECT_2 + IS_ALIGNED_TO;
pub(in crate::types) const MAYBE_IS_ALIGNED_AND_NOT_NULL: usize = CALL_3 + MAYBE_IS_ALIGNED + CONST_POINTER_IS_NULL + 9;
pub(in crate::types) const POINTER_PRECONDITION: usize = CHECK_LANGUAGE_UB + CALL_3 + MAYBE_IS_ALIGNED_AND_NOT_NULL + 14;
pub(in crate::types) const NONNULL_PRECONDITION: usize = CHECK_LANGUAGE_UB + CALL_1 + MUT_POINTER_IS_NULL + 8;
pub(in crate::types) const NONNULL_NEW_UNCHECKED_WORK: usize = 2 * CALL_1 + NONNULL_PRECONDITION + 1;
pub(in crate::types) const NONNULL_AS_PTR: usize = 2 * CALL_1 + 2;
pub(in crate::types) const NONNULL_CAST: usize = 2 * CALL_1 + NONNULL_AS_PTR + 3;
pub(in crate::types) const UNIQUE_CAST: usize = CALL_1 + NONNULL_CAST + 3;

// RawVec::ptr traverses RawVecInner::ptr/non_null, Unique::cast/as_non_null_ptr and
// NonNull::cast/as_ptr. This also bounds each shorter thin-pointer accessor used here.
pub(in crate::types) const POINTER_ACCESS: usize = 4 * CALL_1 + UNIQUE_CAST + NONNULL_AS_PTR + 3;
pub(in crate::types) const POINTER_READ: usize = 2 * CALL_1 + POINTER_PRECONDITION + 1;
pub(in crate::types) const POINTER_WRITE: usize = 2 * CALL_2 + POINTER_PRECONDITION + 1;
pub(in crate::types) const CHECKED_MULTIPLY: usize = 4 * CALL_2 + CALL_1 + 16;
pub(in crate::types) const OVERFLOWING_ADD: usize = 2 * CALL_2 + 12;
pub(in crate::types) const POINTER_ADD_WORK: usize = CALL_2 + CHECK_LANGUAGE_UB + 2 * CALL_3 + SELECT_3
    + CHECKED_MULTIPLY + RAW_POINTER_ADDRESS + OVERFLOWING_ADD + 39;
pub(in crate::types) const MANUALLY_DROP: usize = 2 * CALL_1 + 4;
pub(in crate::types) const MANUALLY_DROP_DEREF: usize = 2 * CALL_1 + 4;
pub(in crate::types) const VECTOR_CAPACITY: usize = 4 * CALL_1 + CALL_2 + 14;
pub(in crate::types) const VECTOR_LENGTH: usize = 2 * CALL_1 + 7;
pub(in crate::types) const RAW_SLICE_POINTER: usize = 4 * CALL_2 + 1;
pub(in crate::types) const BOX_FROM_RAW: usize = 4 * CALL_1 + CALL_2 + NONNULL_PRECONDITION + 5;
pub(in crate::types) const BOX_INTO_RAW: usize = CALL_1 + MANUALLY_DROP + 2 * MANUALLY_DROP_DEREF + POINTER_READ + 12;

pub(in crate::types) const NONNULL_NEW: usize = CALL_1 + MUT_POINTER_IS_NULL + NONNULL_NEW_UNCHECKED_WORK + 6;
pub(in crate::types) const NONNULL_SLICE_POINTER: usize = 2 * CALL_2 + NONNULL_AS_PTR + RAW_SLICE_POINTER + NONNULL_NEW_UNCHECKED_WORK;
pub(in crate::types) const NONNULL_SLICE_AS_PTR: usize = 2 * CALL_1 + NONNULL_CAST + NONNULL_AS_PTR;
pub(in crate::types) const RAW_ALLOCATION: usize = CALL_1 + 2 + 6 + 6 + CALL_2 + 2;
pub(in crate::types) const GLOBAL_ALLOCATION_BODY: usize = 6 + RAW_ALLOCATION + NONNULL_NEW + NONNULL_SLICE_POINTER + 23;
pub(in crate::types) const GLOBAL_ALLOCATION: usize = CALL_2 + CALL_3 + SELECT_2 + GLOBAL_ALLOCATION_BODY;
pub(in crate::types) const BOX_ALLOCATION_WORK: usize = 3 * CALL_1 + GLOBAL_ALLOCATION + NONNULL_SLICE_AS_PTR + CALL_2 + 7;

pub(in crate::types) const ALIGNMENT_NEW: usize = 3 * CALL_1 + CHECK_LANGUAGE_UB + IS_POWER_OF_TWO + 7;
pub(in crate::types) const ALIGNMENT_AS_USIZE: usize = 5 * CALL_1 + 2;
pub(in crate::types) const LAYOUT_SIZE_VALID: usize = CALL_2 + CALL_1 + ALIGNMENT_AS_USIZE + CALL_2 + 10;
pub(in crate::types) const LAYOUT_FROM_SIZE_ALIGNMENT: usize = 2 * CALL_2 + LAYOUT_SIZE_VALID + 12;
pub(in crate::types) const LAYOUT_FOR_VALUE: usize = 4 * CALL_1 + ALIGNMENT_NEW + LAYOUT_FROM_SIZE_ALIGNMENT + 17;
pub(in crate::types) const GLOBAL_DEALLOCATION: usize = 2 * CALL_3 + SELECT_2 + CALL_2 + CALL_3 + 4 * CALL_1 + 15;
pub(in crate::types) const BOX_RETIREMENT_WORK: usize = CALL_1 + 2 + 3 * CALL_1 + 3 + LAYOUT_FOR_VALUE
    + 6 + 5 + UNIQUE_CAST + 3 * CALL_1 + 6 + GLOBAL_DEALLOCATION;
// Vec/RawVec/Alignment/NonNull/Unique construction has ten calls, eight arguments,
// and 25 direct field, binding, aggregate and intrinsic events.
pub(in crate::types) const EMPTY_VECTOR_CONSTRUCTION: usize = 3 * 8 + 2 * 10 + 25;
pub(in crate::types) const EMPTY_VECTOR_RETIREMENT: usize = 3 * CALL_1 + POINTER_ACCESS + RAW_SLICE_POINTER
    + 2 * CALL_1 + 4 + 2 * CALL_2 + 2 * CALL_1 + 36;
pub(in crate::types) const EMPTY_VECTOR_WORK: usize = EMPTY_VECTOR_CONSTRUCTION + EMPTY_VECTOR_RETIREMENT;

pub(in crate::types) type FixedQuote = Option<(usize, usize)>;

// Constant initializers use these helpers. Their arrays, loops and arithmetic do not execute
// in a controlled computation; callers separately admit dynamic quote preparation.
pub(in crate::types) const fn fixed_quote<const N: usize>(work: usize, carriers: [(usize, usize); N]) -> FixedQuote {
    let mut bytes = 0usize;
    let mut index = 0usize;
    while index < N {
        let (count, width) = carriers[index];
        let Some(part) = count.checked_mul(width) else { return None };
        let Some(total) = bytes.checked_add(part) else { return None };
        bytes = total;
        index += 1;
    }
    Some((work, bytes))
}

// Every counted event initializes at most one carrier. Complete argument groups and results
// are included, so multiplying by the maximum bounds cumulative transfers. Private RawVec
// representations are bounded by the actual Vec containing them. Sized private references use
// their thin-pointer representation; unsized pointers retain their actual metadata.
pub(in crate::types) const fn event_quote(work: usize, representations: &[usize]) -> FixedQuote {
    let mut width = 0usize;
    let mut index = 0usize;
    while index < representations.len() {
        if representations[index] > width {
            width = representations[index];
        }
        index += 1;
    }
    match work.checked_mul(width) {
        Some(bytes) => Some((work, bytes)),
        None => None,
    }
}

pub(in crate::types) const fn add_quotes(left: FixedQuote, right: FixedQuote) -> FixedQuote {
    let (Some((left_work, left_bytes)), Some((right_work, right_bytes))) = (left, right) else { return None };
    let Some(work) = left_work.checked_add(right_work) else { return None };
    let Some(bytes) = left_bytes.checked_add(right_bytes) else { return None };
    Some((work, bytes))
}

// Layout::array/layout_array and the successful checked-size/alignment path.
pub(in crate::types) const ARRAY_LAYOUT: usize = 6 * CALL_1 + 2 * CALL_2 + 16
    + 3 * CHECK_LANGUAGE_UB + 2 * CHECKED_MULTIPLY
    + 3 * LAYOUT_FROM_SIZE_ALIGNMENT + 3 * ALIGNMENT_AS_USIZE;
// RawVecInner::current_memory: size/capacity selection, layout and pointer reconstruction.
pub(in crate::types) const CURRENT_MEMORY: usize = 6 * CALL_1 + 2 * CALL_2 + POINTER_PRECONDITION + 16
    + LAYOUT_FOR_VALUE + CHECK_LANGUAGE_UB + CHECKED_MULTIPLY;

// Drop of the Vec/RawVec headers and backing only. Element owners are prepaid separately.
pub(in crate::types) const VECTOR_BACKING_RETIREMENT: usize = 3 * CALL_1 + CURRENT_MEMORY + BOX_RETIREMENT_WORK + 12
    + POINTER_ACCESS + RAW_SLICE_POINTER + VECTOR_LENGTH + 2 * CALL_1 + 8;

// copy_nonoverlapping's successful debug checks cover both pointer alignments and
// the non-overlap calculation (addresses, checked byte count and absolute difference).
pub(in crate::types) const COPY_PRECONDITION: usize = CHECK_LANGUAGE_UB + CALL_5
    + 2 * MAYBE_IS_ALIGNED_AND_NOT_NULL + 2 * CALL_4 + CALL_3
    + 2 * RAW_POINTER_ADDRESS + CHECKED_MULTIPLY + 2 * CALL_2 + 26;

// smallvec 1.15.2: inline_capacity/spilled, inline_mut and triple_mut, including the
// MaybeUninit/ManuallyDrop pointer and successful NonNull/Option construction.
pub(in crate::types) const SMALL_INLINE_CAPACITY: usize = CALL_1 + 5;
pub(in crate::types) const SMALL_SPILLED: usize = CALL_1 + SMALL_INLINE_CAPACITY + 3;
pub(in crate::types) const SMALL_INLINE_POINTER: usize = 4 * CALL_1 + NONNULL_NEW + 8;
pub(in crate::types) const SMALL_TRIPLE_MUT: usize = CALL_1 + SMALL_SPILLED + SMALL_INLINE_POINTER + SMALL_INLINE_CAPACITY + 8;
// SmallVec::new, its array-layout assertion and empty union's uninit/assume_init chain:
// nine no-argument calls, three one-argument calls and 24 direct events.
pub(in crate::types) const SMALL_NEW: usize = 9 * 2 + 3 * CALL_1 + 24;
// Both retirement representations are bounded: inline slice drop, or reconstruction of a
// Vec header for a spilled buffer. Nested entries and existing Vec backing are separate.
pub(in crate::types) const SMALL_RETIRE_WRAPPERS: usize = CALL_1 + SMALL_SPILLED + CALL_2 + CALL_1
    + SMALL_TRIPLE_MUT + POINTER_ACCESS + 3 * CALL_2 + 2 * CHECK_LANGUAGE_UB
    + 2 * MAYBE_IS_ALIGNED_AND_NOT_NULL + CHECKED_MULTIPLY + 3 * CALL_1 + 22
    + 8 * CALL_1 + 4 * CALL_2 + NONNULL_NEW_UNCHECKED_WORK + 24;

pub(in crate::types) const fn smallvec_event_quote<T, const N: usize>(work: usize) -> FixedQuote {
    event_quote(work, &[
        size_of::<smallvec::SmallVec<[T; N]>>(), size_of::<[T; N]>(), size_of::<T>(),
        size_of::<(&mut smallvec::SmallVec<[T; N]>, T)>(), size_of::<(*mut T, T)>(),
        size_of::<Vec<T>>(), size_of::<std::mem::MaybeUninit<[T; N]>>(),
        size_of::<std::mem::ManuallyDrop<[T; N]>>(),
        size_of::<(NonNull<T>, &mut usize, usize)>(), size_of::<&mut [T]>(),
        size_of::<Option<NonNull<T>>>(), size_of::<Layout>(),
        size_of::<Result<Layout, smallvec::CollectionAllocErr>>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<(*const (), *const (), usize)>(), size_of::<usize>(), size_of::<bool>(),
    ])
}

/// Quotes an empty SmallVec's scalar checks separately from its owned representations.
/// The pinned union implementation passes uninitialized inline storage through MaybeUninit
/// and transmute wrappers. The containing SmallVec bounds that private union even when its
/// heap variant is wider than the inline array. Work remains the complete SMALL_NEW bound;
/// caller wrappers, transfers and retirement are separate.
pub(in crate::types) const fn smallvec_new_quote<T, const N: usize>() -> FixedQuote {
    // Keep the full event count for scalar carriers. Twenty additional owned carriers cover
    // uninit construction/return (2), three by-value call arguments (9), their results (3),
    // and field initialization, aggregate construction and return for the union and vector (6).
    add_quotes(
        event_quote(SMALL_NEW, &[size_of::<(usize, usize)>()]),
        fixed_quote(0, [(20, size_of::<smallvec::SmallVec<[T; N]>>())]),
    )
}

/// Quotes SmallVec pointer, allocation-metadata and header-retirement phases without moving entries.
pub(in crate::types) const fn smallvec_borrowed_quote<T>(work: usize) -> FixedQuote {
    event_quote(work, &[
        size_of::<Vec<T>>(), size_of::<(NonNull<T>, usize, usize)>(),
        size_of::<(NonNull<T>, &mut usize, usize)>(), size_of::<&mut [T]>(),
        size_of::<Option<NonNull<T>>>(), size_of::<Layout>(),
        size_of::<Result<Layout, smallvec::CollectionAllocErr>>(),
        size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
        size_of::<(*const (), Layout, bool)>(), size_of::<(*const (), NonNull<u8>, Layout)>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<(*mut (), usize, usize, usize)>(), size_of::<usize>(), size_of::<bool>(),
    ])
}

/// Quotes pointer-copy checks with their complete five-field precondition capture.
pub(in crate::types) const fn copy_precondition_quote() -> FixedQuote {
    event_quote(COPY_PRECONDITION, &[
        size_of::<(*const (), *mut (), usize, usize, usize)>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<Option<usize>>(), size_of::<usize>(), size_of::<bool>(),
    ])
}

/// Quotes SmallVec::with_capacity and its header/backing retirement, excluding live entry owners.
pub(in crate::types) fn smallvec_with_capacity_quote<T, const N: usize>(capacity: usize) -> RunResult<(usize, usize)> {
    const RESERVE_EMPTY: usize = CALL_1 + 3 * CALL_2 + SMALL_TRIPLE_MUT + 14;
    let fixed = const { add_quotes(
        smallvec_new_quote::<T, N>(),
        smallvec_borrowed_quote::<T>(RESERVE_EMPTY + SMALL_RETIRE_WRAPPERS),
    ) };
    if size_of::<T>() == 0 || capacity <= N {
        return checked(fixed);
    }
    let layout = Layout::array::<T>(capacity)
        .map_err(|_| RunError::Contract("small vector allocation overflow"))?;
    // try_reserve_exact -> try_grow from an empty inline buffer. The raw allocator call is
    // bounded by the existing Box allocation chain; zero copied entries add no variable work.
    const SPILL: usize = 4 * CALL_2 + SMALL_SPILLED + SMALL_TRIPLE_MUT + SMALL_INLINE_CAPACITY
        + CHECKED_MULTIPLY + LAYOUT_FROM_SIZE_ALIGNMENT + 3 * CALL_1 + 24
        + BOX_ALLOCATION_WORK + NONNULL_NEW + 2 * POINTER_ACCESS
        + 2 * CALL_2 + 20 + VECTOR_BACKING_RETIREMENT;
    add_payload(add_quotes(fixed, const { add_quotes(add_quotes(
        smallvec_borrowed_quote::<T>(SPILL), copy_precondition_quote(),
    ),
        // from_heap constructs and returns the private union before self.data receives it.
        // Its containing SmallVec bounds those three owned representations.
        fixed_quote(0, [(3, size_of::<smallvec::SmallVec<[T; N]>>())]),
    ) }), 0, layout.size())
}

/// Quotes SmallVec::push when a previously admitted slot is available.
/// The caller prepays the inserted entry's shallow retirement and nested owners.
pub(in crate::types) const fn prepared_smallvec_push_quote<T, const N: usize>() -> RunResult<(usize, usize)> {
    checked(const {
        let work = CALL_2 + SMALL_TRIPLE_MUT + POINTER_ACCESS + POINTER_ADD_WORK + POINTER_WRITE + 11;
        let value_group = size_of::<(&mut smallvec::SmallVec<[T; N]>, T)>();
        let write_group = size_of::<(*mut T, T)>();
        let width = if value_group > write_group { value_group } else { write_group };
        // An available slot needs no allocation. Bound pointer work by triple_mut's result
        // and the debug pointer-check captures, without allocator layouts or owned entries.
        let pointers = event_quote(work, &[
            size_of::<(NonNull<T>, &mut usize, usize)>(),
            size_of::<(*const (), usize, usize)>(), size_of::<(*mut (), usize, bool)>(),
            size_of::<Option<usize>>(), size_of::<usize>(), size_of::<bool>(),
        ]);
        // push, ptr::write and write_via_move each transfer their value argument three
        // times; the destination write adds one. Pointer checks never move that value.
        add_quotes(pointers, fixed_quote(0, [(10, width)]))
    })
}

/// Quotes a pair of SmallVec length/capacity reads, including either retained storage representation.
pub(in crate::types) const fn smallvec_metadata_quote<T, const N: usize>() -> RunResult<(usize, usize)> {
    // len/capacity -> triple use the const-pointer counterpart of triple_mut. Its custom
    // ConstNonNull wrapper adds a result map around the same NonNull/pointer checks.
    checked(const { event_quote(2 * (CALL_1 + SMALL_TRIPLE_MUT + 2 * CALL_1 + 8), &[
        size_of::<&smallvec::SmallVec<[T; N]>>(), size_of::<(NonNull<T>, usize, usize)>(),
        size_of::<Option<NonNull<T>>>(), size_of::<(*const (), usize, usize)>(),
        size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
    ]) })
}

pub(in crate::types) const fn checked(quote: FixedQuote) -> RunResult<(usize, usize)> {
    match quote {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("diagnostic buffer quotation overflow")),
    }
}

pub(in crate::types) fn add_payload(quote: FixedQuote, work: usize, bytes: usize) -> RunResult<(usize, usize)> {
    checked(add_quotes(quote, Some((work, bytes))))
}

pub(in crate::types) const fn preparation_quote(work: usize) -> FixedQuote {
    event_quote(work, &[
        size_of::<usize>(),
        size_of::<bool>(),
        size_of::<Layout>(),
        size_of::<Result<Layout, std::alloc::LayoutError>>(),
        size_of::<FixedQuote>(),
        size_of::<RunResult<(usize, usize)>>(),
    ])
}

/// Quotes evaluating the dynamic SmallVec construction quotation before that evaluation runs.
pub(in crate::types) const fn smallvec_quote_preparation() -> RunResult<(usize, usize)> {
    checked(const { preparation_quote(ARRAY_LAYOUT + 8 * CALL_2 + 6 * CALL_1 + 34) })
}

/// Quotes one SmallVec pop, including the empty branch and transfer of a present entry.
/// Entry payload destruction remains with the owner that originally inserted the entry.
pub(in crate::types) const fn smallvec_pop_quote<T, const N: usize>() -> RunResult<(usize, usize)> {
    checked(const {
        let work = CALL_1 + SMALL_TRIPLE_MUT + NONNULL_AS_PTR + POINTER_ADD_WORK
            + POINTER_READ + 15;
        // Pop borrows the vector without allocating or retiring its header. Keep the
        // pointer-check captures separate from the returned entry and Option carriers.
        let pointers = event_quote(work, &[
            size_of::<(NonNull<T>, &mut usize, usize)>(),
            size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
            size_of::<Option<usize>>(), size_of::<usize>(), size_of::<bool>(),
        ]);
        add_quotes(pointers, fixed_quote(0, [
            (4, size_of::<T>()), (4, size_of::<Option<T>>()),
        ]))
    })
}

/// Quotes the fixed reserve_exact path and new backing retirement, excluding Grow's payload.
/// `grew` selects allocation; `was_spilled` selects realloc instead of the first inline spill.
/// The caller separately admits requested storage bytes, live-entry relocation, and entry owners.
pub(in crate::types) const fn smallvec_reserve_exact_quote<T, const N: usize>(
    grew: bool,
    was_spilled: bool,
) -> RunResult<(usize, usize)> {
    const RESERVE: usize = 3 * CALL_2 + CALL_1 + SMALL_TRIPLE_MUT + 14;
    const GROW: usize = CALL_2 + SMALL_SPILLED + SMALL_TRIPLE_MUT
        + SMALL_INLINE_CAPACITY + ARRAY_LAYOUT + 4 * CALL_1 + 24;
    const FIRST_SPILL: usize = CALL_1 + RAW_ALLOCATION + NONNULL_NEW + NONNULL_CAST
        + 2 * NONNULL_AS_PTR + CALL_3 + 12;
    const REALLOC: usize = ARRAY_LAYOUT + 2 * CALL_3 + CALL_4
        + NONNULL_NEW_UNCHECKED_WORK + 2 * CALL_1 + NONNULL_AS_PTR
        + NONNULL_NEW + NONNULL_CAST + 12;
    let quote = smallvec_borrowed_quote::<T>(RESERVE);
    if !grew {
        return checked(quote);
    }
    let branch = if was_spilled { REALLOC } else { FIRST_SPILL };
    let quote = add_quotes(quote, smallvec_borrowed_quote::<T>(
        GROW + branch + VECTOR_BACKING_RETIREMENT,
    ));
    let quote = if was_spilled { quote } else {
        add_quotes(quote, copy_precondition_quote())
    };
    // from_heap creates, returns and installs the private union; its containing SmallVec
    // bounds all three representations without treating the inline width as work.
    checked(add_quotes(quote, fixed_quote(0, [(3, size_of::<smallvec::SmallVec<[T; N]>>())])))
}

/// Quotes selecting and composing the fixed SmallVec reserve quotation.
pub(in crate::types) const fn smallvec_reserve_quote_preparation() -> RunResult<(usize, usize)> {
    checked(const { preparation_quote(6 * CALL_2 + 4 * CALL_1 + 24) })
}
