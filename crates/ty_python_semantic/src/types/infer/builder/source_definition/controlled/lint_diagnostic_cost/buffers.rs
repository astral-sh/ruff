//! Quotations for diagnostic buffers with explicit capacity and unique ownership.

use std::alloc::Layout;
use std::collections::TryReserveError;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, DiagnosticMessage, Severity, Span, SubDiagnostic, UnifiedFile};
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};

use crate::types::local_transfer::collections::{
    ARRAY_LAYOUT, BOX_ALLOCATION_WORK, BOX_FROM_RAW, BOX_INTO_RAW, BOX_RETIREMENT_WORK, CALL_1, CALL_2, CALL_3, CALL_4, CALL_5, CHECKED_MULTIPLY, CHECK_LANGUAGE_UB, COPY_PRECONDITION, CURRENT_MEMORY, EMPTY_VECTOR_WORK, FixedQuote, GLOBAL_ALLOCATION, GLOBAL_DEALLOCATION, LAYOUT_FROM_SIZE_ALIGNMENT, MANUALLY_DROP, MANUALLY_DROP_DEREF, MAYBE_IS_ALIGNED_AND_NOT_NULL, NONNULL_AS_PTR, NONNULL_CAST, NONNULL_NEW, NONNULL_NEW_UNCHECKED_WORK, NONNULL_SLICE_AS_PTR, NONNULL_SLICE_POINTER, POINTER_ACCESS, POINTER_ADD_WORK, POINTER_PRECONDITION, POINTER_READ, POINTER_WRITE, RAW_ALLOCATION, RAW_SLICE_POINTER, SELECT_2, SMALL_INLINE_CAPACITY, SMALL_INLINE_POINTER, SMALL_NEW, SMALL_RETIRE_WRAPPERS, VECTOR_BACKING_RETIREMENT, VECTOR_CAPACITY, VECTOR_LENGTH, add_payload, add_quotes, checked, copy_precondition_quote, event_quote, fixed_quote, preparation_quote, smallvec_borrowed_quote, smallvec_event_quote, smallvec_quote_preparation,
};

pub(in crate::types::infer::builder) use crate::types::local_transfer::collections::{
    prepared_smallvec_push_quote, smallvec_metadata_quote, smallvec_with_capacity_quote,
};

// Vec::with_capacity[_in], RawVec[_Inner] constructors and their successful results.
const VECTOR_NEW: usize = 2 * CALL_1 + 3 * CALL_2 + CALL_3 + CALL_4
    + ARRAY_LAYOUT + VECTOR_CAPACITY + 3 * POINTER_ACCESS + 28
    + CHECK_LANGUAGE_UB + NONNULL_NEW_UNCHECKED_WORK + 4 * CALL_1 + 12;
// Both needs_to_grow calls, the successful reserve result and assert_unchecked.
const RESERVE_NO_GROW: usize = 3 * CALL_3 + 3 * CALL_4 + 2 * VECTOR_CAPACITY + CALL_1 + 12
    + CHECK_LANGUAGE_UB + 6;
// grow_exact/finish_grow, layout equality, pointer/capacity installation and result returns.
const RESERVE_GROW: usize = 3 * CALL_4 + 3 * CALL_3 + 6 * CALL_1
    + ARRAY_LAYOUT + CURRENT_MEMORY + 3 * POINTER_ACCESS + 32
    + CHECK_LANGUAGE_UB + NONNULL_NEW_UNCHECKED_WORK + 4 * CALL_1;
// Global's same-alignment grow path adds its old-layout checks and realloc call to the
// allocation/result path already bounded by BOX_ALLOCATION_WORK.
const GLOBAL_GROW: usize = BOX_ALLOCATION_WORK + CALL_5 + 2 * CALL_4
    + 4 * CALL_3 + 5 * CALL_1 + POINTER_PRECONDITION + 16;
// Vec::push/push_mut with an existing slot, including pointer write and length update.
const PREPARED_PUSH: usize = 2 * CALL_2 + VECTOR_LENGTH + VECTOR_CAPACITY
    + 2 * POINTER_ACCESS + POINTER_ADD_WORK + POINTER_WRITE + 12;
// String::push_str -> extend_from_slice -> slice iterator specialization -> append_elements.
// The caller has prepared capacity; byte copying is the variable term in its public quote.
// Iterator construction and its slice view have additional fixed work in string_push_str_quote.
const STRING_APPEND: usize = 4 * CALL_2 + 3 * CALL_1 + VECTOR_LENGTH + 2 * POINTER_ACCESS
    + POINTER_ADD_WORK + COPY_PRECONDITION + RESERVE_NO_GROW + 20;
// AtomicUsize::fetch_sub and the acquire fence, including successful ordering dispatch.
const ARC_DECREMENT: usize = 2 * CALL_3 + 3 * CALL_1 + CALL_2 + 12;
const ARC_ACQUIRE: usize = 2 * CALL_1 + 8;
const ARC_NEW: usize = CALL_1 + 4 * CALL_1 + BOX_ALLOCATION_WORK
    + MANUALLY_DROP + 3 * POINTER_ACCESS + NONNULL_NEW_UNCHECKED_WORK
    + CALL_1 + CALL_2 + 20;
// Last-strong and implicit-weak retirement. Nested diagnostic fields have their own owners;
// this pays the Arc/Weak handles, field-drop dispatch and shared backing deallocation.
const ARC_RETIREMENT: usize = 2 * ARC_DECREMENT + 2 * ARC_ACQUIRE
    + 7 * POINTER_ACCESS + 8 * CALL_1 + 4 * CALL_2 + BOX_RETIREMENT_WORK + 44;

/// Quotes final Arc/Weak handles and backing retirement, excluding the stored value's owners.
const fn arc_retirement_quote() -> FixedQuote {
    event_quote(ARC_RETIREMENT, &[
        size_of::<(&AtomicUsize, usize, Ordering)>(), size_of::<(usize, Ordering)>(),
        size_of::<(NonNull<u8>, Layout, *const ())>(), size_of::<(Layout, bool, *const ())>(),
        size_of::<(*const (), usize, bool)>(), size_of::<(*const (), usize, usize)>(),
        size_of::<Layout>(), size_of::<NonNull<[u8]>>(), size_of::<usize>(), size_of::<bool>(),
    ])
}

/// Quotes cloning one Name and retiring the clone, including a possible last shared text owner.
/// Cloning shares heap text; neither operation allocates or traverses the text bytes.
pub(in crate::types::infer::builder) const fn name_clone_retirement_quote() -> RunResult<(usize, usize)> {
    // char_str 0.0.4 TextLen tag extraction uses fixed arrays/byte conversions. Header access
    // validates the tag and subtracts its offset before reading the atomic count.
    const TAG: usize = 3 * CALL_1 + 12;
    const EXACT: usize = 2 * CALL_1 + TAG + 5;
    const OFFSET: usize = 3 * CALL_1 + CALL_2 + 8;
    const SUBTRACT_POINTER: usize = POINTER_ADD_WORK + CALL_2 + 8;
    const HEADER: usize = CALL_1 + EXACT + OFFSET + POINTER_ACCESS + SUBTRACT_POINTER + CALL_1 + 8;
    const COUNT: usize = CALL_1 + EXACT + HEADER + 5;
    // Name/CharStr/Repr clone, heap tag/cast, count increment and successful overflow check,
    // followed by ptr::read for all three inline/static/heap representations.
    const CLONE: usize = 6 * CALL_1 + 16 + COUNT + ARC_DECREMENT + POINTER_READ;
    // Both length representations are bounded, including the 32-bit heap-stored length.
    const LENGTH: usize = 5 * CALL_1 + EXACT + TAG + OFFSET + 2 * SUBTRACT_POINTER
        + 2 * POINTER_ACCESS + POINTER_READ + 18;
    // Capacity validation, layout wrappers, two checked additions, optional heap-length word,
    // alignment selection and Layout::from_size_align's successful checked result.
    const TEXT_LAYOUT: usize = LENGTH + HEADER + 8 * CALL_1 + 6 * CALL_2 + OFFSET
        + LAYOUT_FROM_SIZE_ALIGNMENT + 35;
    const ALLOCATION_POINTER: usize = EXACT + HEADER + 6 * CALL_1 + OFFSET
        + 2 * SUBTRACT_POINTER + POINTER_ACCESS + 14;
    // CharStr/Repr release, count decrement, acquire fence, final layout and allocator release.
    const RETIRE: usize = 7 * CALL_1 + 21 + COUNT + ARC_DECREMENT + ARC_ACQUIRE
        + TEXT_LAYOUT + ALLOCATION_POINTER + GLOBAL_DEALLOCATION;
    checked(event_quote(CLONE + RETIRE, &[
        size_of::<ruff_python_ast::name::Name>(), size_of::<&ruff_python_ast::name::Name>(),
        size_of::<AtomicUsize>(), size_of::<Ordering>(), size_of::<[usize; 2]>(),
        size_of::<Layout>(), size_of::<Result<Layout, std::alloc::LayoutError>>(),
        size_of::<(NonNull<u8>, usize)>(), size_of::<(*const (), NonNull<u8>, Layout)>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<Option<usize>>(), size_of::<usize>(), size_of::<bool>(),
    ]))
}

/// Quotes Vec::shrink_to_fit followed by SmallVec::from_vec, preserving prepaid element owners.
/// Existing Vec backing retirement transfers to the SmallVec when it remains spilled.
pub(in crate::types::infer::builder) fn vec_into_smallvec_quote<T, const N: usize>(len: usize, capacity: usize) -> RunResult<(usize, usize)> {
    if len > capacity {
        return Err(RunError::Contract("small vector length exceeds capacity"));
    }
    let shrink = Some(vec_shrink_storage_quote::<T>(len, capacity)?);
    // The shrink test and both from_vec paths: empty inline storage, copying/zeroing source
    // length, or forgetting the Vec and transferring its heap pointer/capacity/length.
    const CONVERT: usize = CALL_1 + VECTOR_LENGTH + VECTOR_CAPACITY + 6
        + CALL_1 + VECTOR_CAPACITY + SMALL_INLINE_CAPACITY + SMALL_NEW
        + 2 * VECTOR_LENGTH + VECTOR_CAPACITY + 3 * POINTER_ACCESS + SMALL_INLINE_POINTER
        + COPY_PRECONDITION + 5 * CALL_1 + 3 * CALL_2 + NONNULL_NEW + 28
        + SMALL_RETIRE_WRAPPERS;
    let fixed = add_quotes(shrink, const {
        const BORROWED: usize = VECTOR_LENGTH + VECTOR_CAPACITY
            + VECTOR_CAPACITY + SMALL_INLINE_CAPACITY
            + 2 * VECTOR_LENGTH + VECTOR_CAPACITY + 3 * POINTER_ACCESS + SMALL_INLINE_POINTER
            + NONNULL_NEW + SMALL_RETIRE_WRAPPERS;
        add_quotes(add_quotes(
            smallvec_event_quote::<T, N>(CONVERT - BORROWED - COPY_PRECONDITION),
            smallvec_borrowed_quote::<T>(BORROWED),
        ), copy_precondition_quote())
    });
    if size_of::<T>() == 0 || len <= N {
        let copied = len.checked_mul(size_of::<T>())
            .ok_or(RunError::Contract("inline small vector transfer overflow"))?;
        add_payload(fixed, len, copied)
    } else {
        checked(fixed)
    }
}

/// Quotes Arc construction with separate stored-value and allocator/handle transfers.
/// `layout` includes the reference counts and sized value in the complete padded ArcInner
/// allocation used with Global. Backing allocation bytes, nested value owners and eventual
/// retirement are separate.
const fn arc_construction_quote(layout: Layout) -> FixedQuote {
    const BORROWED: usize = GLOBAL_ALLOCATION + NONNULL_SLICE_AS_PTR + MANUALLY_DROP
        + 3 * POINTER_ACCESS + NONNULL_NEW_UNCHECKED_WORK;
    // Box's intrinsic write receives both the pointer and the complete stored value.
    let Ok((write_arguments, _)) = layout.extend(Layout::new::<*mut ()>()) else { return None; };
    let owned = event_quote(ARC_NEW - BORROWED, &[
        write_arguments.pad_to_align().size(), size_of::<Box<()>>(), size_of::<Arc<()>>(),
        size_of::<AtomicUsize>(), size_of::<Ordering>(), size_of::<usize>(), size_of::<bool>(),
    ]);
    let borrowed = const { event_quote(BORROWED, &[
        size_of::<Layout>(), size_of::<NonNull<[u8]>>(),
        size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
        size_of::<(*const (), Layout, bool)>(), size_of::<(*const (), NonNull<u8>, Layout)>(),
        size_of::<(*const (), usize, bool)>(), size_of::<std::mem::ManuallyDrop<Box<()>>>(),
        size_of::<Box<()>>(), size_of::<Arc<()>>(), size_of::<NonNull<()>>(),
        size_of::<usize>(), size_of::<bool>(),
    ]) };
    add_quotes(owned, borrowed)
}

/// Quotes an Arc allocation and its last strong/weak retirement using the owning module's layout.
/// The layout includes the complete padded ArcInner for a sized value and Global allocator.
/// Nested fields must already carry their own construction and retirement admission.
pub(in crate::types::infer::builder) const fn arc_owner_quote(layout: Layout) -> RunResult<(usize, usize)> {
    checked(add_quotes(arc_construction_quote(layout),
        add_quotes(const { arc_retirement_quote() }, Some((0, layout.size())))))
}

/// Quotes a fixed Box allocation, by-value transfers and backing retirement, excluding nested owners.
pub(in crate::types::infer::builder) const fn fixed_box_quote<T>() -> RunResult<(usize, usize)> {
    checked(const { add_quotes(add_quotes(
        event_quote(BOX_ALLOCATION_WORK + CALL_1 + 4, &[
            size_of::<T>(), size_of::<(*mut T, T)>(), size_of::<Box<T>>(), size_of::<Layout>(),
            size_of::<NonNull<[u8]>>(), size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
            size_of::<(*const (), Layout, bool)>(), size_of::<(*const (), NonNull<u8>, Layout)>(),
            size_of::<*mut u8>(), size_of::<usize>(), size_of::<bool>(),
        ]),
        event_quote(BOX_RETIREMENT_WORK, &[
            size_of::<Box<T>>(), size_of::<Layout>(),
            size_of::<(*const (), NonNull<u8>, Layout)>(), size_of::<(*const (), usize, bool)>(),
            size_of::<(*const (), usize, usize)>(), size_of::<usize>(), size_of::<bool>(),
        ]),
    ), Some((0, size_of::<T>()))) })
}

/// Quotes constructing and retiring a SmallVec containing one inline entry, excluding the entry's own construction and retirement.
pub(in crate::types::infer::builder) const fn single_inline_smallvec_quote<T>() -> RunResult<(usize, usize)> {
    // from_const, MaybeUninit::new and SmallVecData::from_const include two
    // ManuallyDrop::new -> MaybeDangling::new chains: seven calls and seven aggregates.
    const CONSTRUCT: usize = 7 * CALL_1 + 7;
    // Two spilled/inline_capacity selections; IndexMut -> DerefMut -> triple_mut;
    // inline_mut's MaybeUninit/ManuallyDrop pointer, NonNull and successful Option unwrap.
    const SELECT: usize = 2 * (3 * CALL_1 + 7) + CALL_2 + 5 * CALL_1
        + NONNULL_NEW + 3 * CALL_1 + 14;
    // from_raw_parts_mut validates the pointer and allocation size before forming the slice.
    // RangeFull keeps that slice; drop_in_place checks its pointer before dropping the entry.
    const RETIRE: usize = CALL_1 + SELECT + POINTER_ACCESS + 3 * CALL_2
        + 2 * CHECK_LANGUAGE_UB + 2 * MAYBE_IS_ALIGNED_AND_NOT_NULL
        + CHECKED_MULTIPLY + 3 * CALL_1 + 22;
    checked(const { add_quotes(event_quote(CONSTRUCT, &[
        size_of::<smallvec::SmallVec<[T; 1]>>(), size_of::<[T; 1]>(),
        size_of::<std::mem::MaybeUninit<[T; 1]>>(), size_of::<std::mem::ManuallyDrop<[T; 1]>>(),
        size_of::<(NonNull<T>, &mut usize, usize)>(), size_of::<&mut [T]>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<Option<NonNull<T>>>(), size_of::<usize>(), size_of::<bool>(),
    ]), smallvec_borrowed_quote::<T>(RETIRE)) })
}

/// Quotes constructing and retiring an empty Vec header without allocating backing storage.
///
/// Later pushes or reservations separately fund stored values and any new backing owner.
pub(in crate::types::infer::builder) const fn empty_vec_quote<T>() -> RunResult<(usize, usize)> {
    checked(event_quote(EMPTY_VECTOR_WORK, &[
        size_of::<Vec<T>>(),
        size_of::<(*mut T, usize)>(),
        size_of::<(*const (), usize, bool)>(),
        size_of::<usize>(),
        size_of::<bool>(),
    ]))
}

/// Quotes buffer-header construction, allocation wrappers and eventual backing retirement.
///
/// A private RawVec is bounded by its containing Vec representation. Elements are not
/// constructed here, and their retirement remains the responsibility of later insertions.
pub(in crate::types::infer::builder) fn vector_with_capacity_quote<T>(capacity: usize) -> RunResult<(usize, usize)> {
    let layout = Layout::array::<T>(capacity)
        .map_err(|_| RunError::Contract("diagnostic vector layout overflow"))?;
    let fixed = const {
        let owners = event_quote(VECTOR_NEW + VECTOR_BACKING_RETIREMENT, &[
            size_of::<Vec<T>>(), size_of::<Layout>(),
            size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
            size_of::<Option<(NonNull<u8>, Layout)>>(),
            size_of::<(Layout, bool, *const ())>(),
            size_of::<(NonNull<u8>, Layout, *const ())>(),
            size_of::<(*const (), usize, bool)>(),
            size_of::<NonNull<[u8]>>(), size_of::<*mut u8>(), size_of::<usize>(), size_of::<bool>(),
        ]);
        // Allocation's pointer and layout subchains do not carry the allocator's full
        // receiver/Layout/zeroed argument group through every nested operation.
        let allocation = add_quotes(add_quotes(
            event_quote(BOX_ALLOCATION_WORK - RAW_ALLOCATION - NONNULL_NEW - NONNULL_SLICE_POINTER, &[
                size_of::<Vec<T>>(), size_of::<Layout>(),
                size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
                size_of::<(Layout, bool, *const ())>(),
                size_of::<(NonNull<u8>, Layout, *const ())>(),
                size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
            ]),
            event_quote(RAW_ALLOCATION, &[
                size_of::<Layout>(), size_of::<(usize, usize)>(), size_of::<*mut u8>(),
            ]),
        ), add_quotes(
            event_quote(NONNULL_NEW, &[
                size_of::<*mut u8>(), size_of::<NonNull<u8>>(), size_of::<Option<NonNull<u8>>>(),
                size_of::<(*const u8,)>(), size_of::<usize>(), size_of::<bool>(),
            ]),
            event_quote(NONNULL_SLICE_POINTER, &[
                size_of::<(NonNull<u8>, usize)>(), size_of::<(*mut u8, usize)>(),
                size_of::<NonNull<[u8]>>(), size_of::<*mut [u8]>(),
                size_of::<(*const (), usize)>(), size_of::<usize>(), size_of::<bool>(),
            ]),
        ));
        // RawVecInner::with_capacity_in also calls needs_to_grow with its receiver,
        // length, capacity and Layout: retain all three complete argument transfers.
        add_quotes(add_quotes(owners, allocation), fixed_quote(0, [
            (3, size_of::<(*const (), usize, usize, Layout)>()),
        ]))
    };
    add_payload(fixed, 0, layout.size())
}

/// Quotes a fresh String with the requested capacity, including its backing's retirement.
///
/// Appending text and converting the completed buffer to a DiagnosticMessage have separate
/// quotes. The payload bytes are the request to the allocator, not logical work units.
pub(in crate::types::infer::builder) fn string_with_capacity_quote(capacity: usize) -> RunResult<(usize, usize)> {
    let vector = Some(vector_with_capacity_quote::<u8>(capacity)?);
    checked(add_quotes(vector, const { event_quote(CALL_1 + 3, &[size_of::<String>()]) }))
}

/// Quotes appending a UTF-8 fragment to a String with enough previously admitted capacity.
///
/// The specialized slice path copies each byte once and cannot grow the buffer under this
/// precondition. The resulting String owner keeps the buffer's prepaid retirement.
pub(in crate::types::infer::builder) fn string_push_str_quote(fragment_len: usize) -> RunResult<(usize, usize)> {
    const FIXED: (FixedQuote, FixedQuote) = {
        const WRAPPERS: usize = STRING_APPEND - VECTOR_LENGTH - 2 * POINTER_ACCESS
            - POINTER_ADD_WORK - COPY_PRECONDITION - RESERVE_NO_GROW;
        const SLICE_LENGTH: usize = 3 * CALL_1 + 2;
        const NONNULL_FROM_REF: usize = 2 * CALL_1 + 3;
        const ITERATOR_NEW: usize = CALL_1 + SLICE_LENGTH + NONNULL_FROM_REF + NONNULL_CAST
            + NONNULL_AS_PTR + POINTER_ADD_WORK + 20;
        const UNSIGNED_OFFSET: usize = 6 * CALL_2 + 2 * NONNULL_AS_PTR + CHECK_LANGUAGE_UB
            + SELECT_2 + (2 + 1) + 32;
        const ITERATOR_LENGTH: usize = CALL_1 + UNSIGNED_OFFSET + 10;
        const VALID_SLICE_SIZE: usize = CALL_2 + 18;
        const SLICE_FROM_RAW: usize = CALL_2 + CHECK_LANGUAGE_UB + CALL_4
            + MAYBE_IS_ALIGNED_AND_NOT_NULL + VALID_SLICE_SIZE + 2 * (2 + 1)
            + RAW_SLICE_POINTER + 20;
        const MAKE_SLICE: usize = CALL_1 + NONNULL_AS_PTR + ITERATOR_LENGTH + SLICE_FROM_RAW + 4;
        // as_bytes calls transmute in addition to the existing wrapper entries.
        let wrappers = event_quote(WRAPPERS + CALL_1, &[
            size_of::<(&mut String, &str)>(),
            size_of::<(&mut Vec<u8>, std::slice::Iter<'_, u8>)>(),
            size_of::<(&mut Vec<u8>, *const [u8])>(),
            size_of::<(*const u8, *mut u8, usize)>(),
        ]);
        let access = add_quotes(
            event_quote(VECTOR_LENGTH, &[
                size_of::<&Vec<u8>>(), size_of::<usize>(),
            ]),
            event_quote(POINTER_ACCESS, &[
                size_of::<*const [u8]>(), size_of::<usize>(),
            ]),
        );
        let destination = event_quote(POINTER_ACCESS, &[
            size_of::<*mut u8>(), size_of::<usize>(),
        ]);
        let offset = event_quote(POINTER_ADD_WORK, &[
            size_of::<(*const (), usize, usize)>(), size_of::<(usize, bool)>(),
            size_of::<Option<usize>>(), size_of::<usize>(), size_of::<bool>(),
        ]);
        // RawVec's no-grow calls retain their full pointer/length/additional/Layout group.
        let reserve = event_quote(RESERVE_NO_GROW, &[
            size_of::<(*const (), usize, usize, Layout)>(), size_of::<(usize, usize)>(),
            size_of::<usize>(), size_of::<bool>(),
        ]);
        // The five copy-check arguments precede narrower alignment/null and overlap checks.
        let check_gate = event_quote(CHECK_LANGUAGE_UB, &[size_of::<usize>(), size_of::<bool>()]);
        let check_arguments = event_quote(CALL_5, &[
            size_of::<(*const (), *mut (), usize, usize, usize)>(),
        ]);
        let alignment = event_quote(2 * MAYBE_IS_ALIGNED_AND_NOT_NULL, &[
            size_of::<(*const (), usize, bool)>(), size_of::<(*const (), usize)>(),
            size_of::<usize>(), size_of::<bool>(),
        ]);
        let overlap = event_quote(COPY_PRECONDITION - CHECK_LANGUAGE_UB - CALL_5
            - 2 * MAYBE_IS_ALIGNED_AND_NOT_NULL, &[
            size_of::<(*const (), *const (), usize, usize)>(), size_of::<(usize, bool)>(),
            size_of::<Option<usize>>(), size_of::<usize>(), size_of::<bool>(),
        ]);
        let copy_calls = event_quote(2 * CALL_3, &[
            size_of::<(*const u8, *mut u8, usize)>(),
        ]);
        // Creating the source iterator calculates its end with a separate pointer addition.
        let iterator_new = add_quotes(event_quote(ITERATOR_NEW - POINTER_ADD_WORK, &[
            size_of::<std::slice::Iter<'_, u8>>(), size_of::<NonNull<[u8]>>(),
            size_of::<&[u8]>(), size_of::<usize>(), size_of::<bool>(),
        ]), offset);
        // Viewing that iterator as a slice checks both its pointer distance and slice validity.
        let iterator_view = add_quotes(event_quote(MAKE_SLICE - SLICE_FROM_RAW, &[
            size_of::<(*const u8, *const u8)>(), size_of::<&[u8]>(),
            size_of::<usize>(), size_of::<bool>(),
        ]), add_quotes(add_quotes(check_gate, event_quote(CALL_4, &[
            size_of::<(*mut (), usize, usize, usize)>(),
        ])), add_quotes(event_quote(MAYBE_IS_ALIGNED_AND_NOT_NULL, &[
            size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
        ]), event_quote(SLICE_FROM_RAW - CHECK_LANGUAGE_UB - CALL_4
            - MAYBE_IS_ALIGNED_AND_NOT_NULL, &[
            size_of::<(*const u8, usize)>(), size_of::<&[u8]>(),
            size_of::<(usize, usize)>(), size_of::<usize>(), size_of::<bool>(),
        ]))));
        let empty = add_quotes(add_quotes(wrappers, access),
            add_quotes(reserve, add_quotes(iterator_new, iterator_view)));
        let nonempty = add_quotes(empty,
            add_quotes(add_quotes(destination, offset), add_quotes(copy_calls,
                add_quotes(add_quotes(check_gate, check_arguments), add_quotes(alignment, overlap)))));
        (empty, nonempty)
    };
    // append_elements always prepares its slice and reserves, but only a nonempty slice
    // forms the destination pointer and invokes the copy and its checks.
    let fixed = if fragment_len == 0 { FIXED.0 } else { FIXED.1 };
    add_payload(fixed, fragment_len, fragment_len)
}

/// Quotes pushing one value into a Vec with an already available slot.
///
/// The quote includes value transfers and the reserved-slot write. The caller must separately
/// fund the newly live entry's shallow retirement and the construction and retirement of any
/// nested storage owned by the value.
pub(in crate::types::infer::builder) const fn prepared_vec_push_quote<T>() -> RunResult<(usize, usize)> {
    checked(const {
        let value_group = size_of::<(&mut Vec<T>, T)>();
        let write_group = size_of::<(*mut T, T)>();
        let width = if value_group > write_group { value_group } else { write_group };
        let pointers = event_quote(PREPARED_PUSH, &[
            size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
            size_of::<&mut Vec<T>>(), size_of::<*mut T>(), size_of::<usize>(), size_of::<bool>(),
        ]);
        // push, push_mut, ptr::write and write_via_move each have three value-argument
        // transfers; the destination write adds one. The full argument groups include padding.
        add_quotes(pointers, fixed_quote(0, [(13, width)]))
    })
}

/// Quotes constructing an annotation with an owned message from a cloned ty-file span.
///
/// The source span contains a copyable ty File, so cloning it does not clone a ruff
/// SourceFile. The quote includes the empty tag vector and eventual shallow annotation
/// retirement. Inserting it and retiring its already owned message are paid separately.
pub(in crate::types::infer::builder) const fn annotation_with_ty_span_quote() -> RunResult<(usize, usize)> {
    checked(const { add_quotes(
        event_quote(
            // Derived span/file/range clones; with_range -> with_optional_range.
            8 * CALL_1 + 2 * CALL_2 + 22
            // Primary/secondary construction, message conversion and field transfers.
            + 2 * CALL_1 + CALL_2 + 16
            // Passive span/message/role fields on disposal.
            + 12,
            &[
                size_of::<Span>(), size_of::<UnifiedFile>(), size_of::<TextRange>(),
                size_of::<Option<TextRange>>(), size_of::<Annotation>(),
                size_of::<DiagnosticMessage>(), size_of::<Option<DiagnosticMessage>>(),
                size_of::<(Span, TextRange)>(), size_of::<(Span, Option<TextRange>)>(),
                size_of::<(Annotation, DiagnosticMessage)>(),
                size_of::<bool>(), size_of::<usize>(),
            ],
        ),
        // Empty tag storage never moves the enclosing Annotation through its pointer checks.
        // Both annotation roles create their tag storage with Vec::new().
        match empty_vec_quote::<ruff_db::diagnostic::DiagnosticTag>() {
            Ok(quote) => Some(quote),
            Err(_) => None,
        },
    ) })
}

/// Quotes Vec::reserve_exact for the supplied live length, capacity and additional slots.
///
/// When growth is needed, this pays the exact requested replacement backing, possible
/// relocation of the old capacity, and backing retirement. Relocation work counts slots;
/// their representation width contributes only bytes. Existing elements keep their owners.
pub(in crate::types::infer::builder) fn vec_reserve_exact_quote<T>(len: usize, capacity: usize, additional: usize) -> RunResult<(usize, usize)> {
    let required = len.checked_add(additional)
        .ok_or(RunError::Contract("diagnostic vector capacity overflow"))?;
    if len > capacity {
        return Err(RunError::Contract("diagnostic vector length exceeds capacity"));
    }
    let fixed = const { event_quote(
        RESERVE_NO_GROW,
        &[
            size_of::<&mut Vec<T>>(),
            size_of::<(*const (), usize, usize, Layout)>(),
            size_of::<Layout>(),
            size_of::<usize>(),
            size_of::<bool>(),
        ],
    ) };
    if required <= capacity || size_of::<T>() == 0 {
        return checked(fixed);
    }
    let new_layout = Layout::array::<T>(required)
        .map_err(|_| RunError::Contract("diagnostic vector allocation overflow"))?;
    let old_layout = Layout::array::<T>(capacity)
        .map_err(|_| RunError::Contract("diagnostic vector relocation overflow"))?;
    let growth = const { add_quotes(add_quotes(
        // RawVec's helper calls carry at most its four-field reserve arguments or
        // the receiver/pointer/two-layout group at its allocator-call boundary.
        event_quote(RESERVE_GROW, &[
            size_of::<Vec<T>>(),
            size_of::<(*const (), usize, usize, Layout)>(),
            size_of::<(*const (), NonNull<[u8]>, usize)>(),
            size_of::<(*const (), NonNull<u8>, Layout, Layout)>(),
            size_of::<Layout>(),
            size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
            size_of::<Option<(NonNull<u8>, Layout)>>(),
            size_of::<(*const (), usize, bool)>(),
            size_of::<usize>(),
            size_of::<bool>(),
        ]),
        // Global's grow implementation adds the zeroing flag to that complete group.
        event_quote(GLOBAL_GROW, &[
            size_of::<Layout>(),
            size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
            size_of::<(Layout, bool, *const ())>(),
            size_of::<(NonNull<u8>, Layout, *const ())>(),
            size_of::<(NonNull<u8>, Layout, Layout, bool)>(),
            size_of::<(*const (), NonNull<u8>, Layout, Layout, bool)>(),
            size_of::<(*const (), usize, bool)>(),
            size_of::<NonNull<[u8]>>(),
            size_of::<*mut u8>(),
            size_of::<usize>(),
            size_of::<bool>(),
        ]),
    ),
        // Final backing retirement uses the allocator's pointer/Layout group.
        event_quote(VECTOR_BACKING_RETIREMENT, &[
            size_of::<Vec<T>>(),
            size_of::<Layout>(),
            size_of::<Option<(NonNull<u8>, Layout)>>(),
            size_of::<(*const (), NonNull<u8>, Layout)>(),
            size_of::<(*const (), usize, bool)>(),
            size_of::<&mut [T]>(),
            size_of::<NonNull<[u8]>>(),
            size_of::<usize>(),
            size_of::<bool>(),
        ]),
    ) };
    let bytes = new_layout.size().checked_add(old_layout.size())
        .ok_or(RunError::Contract("diagnostic vector growth quotation overflow"))?;
    add_payload(add_quotes(fixed, growth), capacity, bytes)
}

/// Quotes a unique Diagnostic with prepared annotation and subdiagnostic capacities.
///
/// The headline is already owned and has prepaid retirement. Each requested vector is empty
/// on construction. Later content operations fund their payloads; this quote funds the fixed
/// Arc allocation/handle chain, empty buffer owners and the diagnostic's final Arc retirement.
pub(in crate::types::infer::builder) fn diagnostic_with_capacity_quote(annotation_capacity: usize, subdiagnostic_capacity: usize) -> RunResult<(usize, usize)> {
    let fixed = const { diagnostic_fixed_quote() };
    let annotations = Some(vector_with_capacity_quote::<Annotation>(annotation_capacity)?);
    let subdiagnostics = Some(vector_with_capacity_quote::<SubDiagnostic>(subdiagnostic_capacity)?);
    checked(add_quotes(add_quotes(fixed, annotations), subdiagnostics))
}

/// Computes the fixed constructor/Arc-owner quote without running layout work during admission.
const fn diagnostic_fixed_quote() -> FixedQuote {
    let Ok(layout) = Diagnostic::allocation_layout() else { return None; };
    let fixed = event_quote(
        2 * CALL_5 + 2 * CALL_1 + 20,
        &[
            // The owning layout bounds each private DiagnosticInner transfer.
            // The allocation itself is charged separately below.
            layout.size(),
            size_of::<(DiagnosticId, Severity, DiagnosticMessage, usize, usize)>(),
            size_of::<Diagnostic>(),
            size_of::<AtomicUsize>(),
            size_of::<Ordering>(),
            size_of::<Layout>(),
            size_of::<NonNull<[u8]>>(),
            size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
            size_of::<*mut u8>(),
            size_of::<usize>(),
            size_of::<bool>(),
        ],
    );
    add_quotes(add_quotes(fixed, arc_construction_quote(layout)),
        add_quotes(arc_retirement_quote(), Some((0, layout.size()))))
}

/// Selects the finite quote calculation performed before a buffer or diagnostic mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types::infer::builder) enum BufferQuotePreparation {
    VectorWithCapacity,
    SmallVecWithCapacity,
    VecIntoSmallVec,
    StringWithCapacity,
    StringPushStr,
    PreparedVecPush,
    VecReserveExact,
    VecIntoBoxedSlice,
    DiagnosticWithCapacity,
}

/// Quotes evaluating one buffer quotation, excluding caller-owned collection/metadata reads.
///
/// Fixed carrier envelopes are constant-evaluated. The selected work covers only checked
/// dynamic layouts, additions, branches and result handling in the corresponding helper.
/// Call `buffer_quote_preparation` with a compile-time-known operation in an inline const
/// block so preparing this quote finishes before runtime admission starts.
pub(in crate::types::infer::builder) const fn buffer_quote_preparation(operation: BufferQuotePreparation) -> RunResult<(usize, usize)> {
    const STRING_NEW: FixedQuote = preparation_quote(ARRAY_LAYOUT + 4 * CALL_2 + 4 * CALL_1 + 16);
    const STRING_APPEND: FixedQuote = add_quotes(
        preparation_quote(2 * CALL_3 + 2 * CALL_1 + 12),
        event_quote(6, &[size_of::<(FixedQuote, FixedQuote)>(), size_of::<FixedQuote>(), size_of::<usize>(), size_of::<bool>()]),
    );
    const VECTOR_PUSH: FixedQuote = preparation_quote(CALL_1 + 4);
    const VECTOR_RESERVE: FixedQuote = preparation_quote(2 * ARRAY_LAYOUT + 4 * CALL_3 + 4 * CALL_1 + 24);
    const VECTOR_BOX: FixedQuote = preparation_quote(2 * ARRAY_LAYOUT + 6 * CALL_3 + 6 * CALL_1 + 40);
    const SMALL_VECTOR_CONVERSION: FixedQuote = preparation_quote(2 * ARRAY_LAYOUT + 9 * CALL_3 + 9 * CALL_1 + 58);
    const DIAGNOSTIC_NEW: FixedQuote = preparation_quote(2 * ARRAY_LAYOUT + 6 * CALL_2 + 8 * CALL_1 + 32);
    checked(match operation {
        BufferQuotePreparation::VectorWithCapacity => STRING_NEW,
        BufferQuotePreparation::SmallVecWithCapacity => return smallvec_quote_preparation(),
        BufferQuotePreparation::VecIntoSmallVec => SMALL_VECTOR_CONVERSION,
        BufferQuotePreparation::StringWithCapacity => STRING_NEW,
        BufferQuotePreparation::StringPushStr => STRING_APPEND,
        BufferQuotePreparation::PreparedVecPush => VECTOR_PUSH,
        BufferQuotePreparation::VecReserveExact => VECTOR_RESERVE,
        BufferQuotePreparation::VecIntoBoxedSlice => VECTOR_BOX,
        BufferQuotePreparation::DiagnosticWithCapacity => DIAGNOSTIC_NEW,
    })
}

/// Quotes consuming a Vec into a boxed slice, including spare-capacity shrink and box retirement.
///
/// Element values and their eventual destruction must already be admitted. A nonempty shrink
/// may request replacement storage and move each live element. An empty shrink deallocates
/// the old backing; zero-sized elements never request backing storage.
pub(in crate::types::infer::builder) fn vec_into_boxed_slice_quote<T>(len: usize, capacity: usize) -> RunResult<(usize, usize)> {
    if len > capacity {
        return Err(RunError::Contract("boxed vector length exceeds capacity"));
    }
    // Vec::into_boxed_slice, its no-shrink test, RawVec::into_box and assume_init.
    const CONVERSION: usize = 3 * CALL_1 + VECTOR_CAPACITY + MANUALLY_DROP + POINTER_READ
        + 2 * MANUALLY_DROP_DEREF + VECTOR_LENGTH + 11
        + CALL_2 + VECTOR_CAPACITY + MANUALLY_DROP + 2 * MANUALLY_DROP_DEREF
        + POINTER_ACCESS + CALL_1 + 2 + RAW_SLICE_POINTER + POINTER_READ + BOX_FROM_RAW + 11
        + CALL_1 + BOX_INTO_RAW + BOX_FROM_RAW + 3;
    let conversion = const { event_quote(CONVERSION + BOX_RETIREMENT_WORK, &[
        size_of::<Vec<T>>(), size_of::<Box<[T]>>(), size_of::<Layout>(),
        size_of::<(*mut T, usize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<NonNull<[u8]>>(), size_of::<usize>(), size_of::<bool>(),
    ]) };
    let shrink = Some(vec_shrink_storage_quote::<T>(len, capacity)?);
    checked(add_quotes(conversion, shrink))
}

// Shared Vec/RawVec shrink body after the caller's length/capacity test.
fn vec_shrink_storage_quote<T>(len: usize, capacity: usize) -> RunResult<(usize, usize)> {
    if len == capacity {
        return Ok((0, 0));
    }
    // Vec/RawVec/RawVecInner shrink wrappers and RawVecInner::shrink_unchecked.
    const SHRINK: usize = 3 * CALL_2 + 2 * CALL_3 + CURRENT_MEMORY + 2 * VECTOR_CAPACITY
        + CHECKED_MULTIPLY + CHECK_LANGUAGE_UB + 2 * CALL_2 + 8
        + LAYOUT_FROM_SIZE_ALIGNMENT + 3 * CALL_1 + 3 * POINTER_ACCESS + NONNULL_NEW_UNCHECKED_WORK + 38;
    let shrink = const { event_quote(SHRINK, &[
        size_of::<Vec<T>>(), size_of::<Layout>(),
        size_of::<Option<(NonNull<u8>, Layout)>>(),
        size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
        size_of::<(NonNull<u8>, Layout, Layout, bool)>(),
        size_of::<usize>(), size_of::<bool>(),
    ]) };
    if size_of::<T>() == 0 {
        return checked(shrink);
    }
    if len == 0 {
        let deallocation = const { event_quote(GLOBAL_DEALLOCATION + 3 * CALL_1 + 10, &[
            size_of::<Layout>(), size_of::<NonNull<u8>>(), size_of::<usize>(),
        ]) };
        return checked(add_quotes(shrink, deallocation));
    }
    let layout = Layout::array::<T>(len)
        .map_err(|_| RunError::Contract("boxed vector allocation overflow"))?;
    // Global::shrink selects its same-alignment realloc branch. GLOBAL_GROW includes
    // the identical realloc/result path, and bounds the shorter shrink dispatch too.
    let allocation = const { event_quote(GLOBAL_GROW, &[
        size_of::<(*const (), NonNull<u8>, Layout, Layout, bool)>(),
        size_of::<Result<NonNull<[u8]>, TryReserveError>>(),
        size_of::<Layout>(), size_of::<NonNull<[u8]>>(), size_of::<usize>(),
    ]) };
    let bytes = layout.size().checked_mul(2)
        .ok_or(RunError::Contract("boxed vector relocation quotation overflow"))?;
    add_payload(add_quotes(shrink, allocation), len, bytes)
}
