//! Work, transfer and owner quotations for checked type-variable defaults.

use std::alloc::Layout;
use std::cell::{Cell, OnceCell};
use std::convert::Infallible;
use std::ops::ControlFlow;
use std::rc::Rc;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::super::lint_diagnostic_cost::arc_owner_quote;
use super::super::storage::slots;
use crate::types::constraints::control::{GrowthPlan, TddError, hash_slots, sequence_growth};
use crate::types::cyclic::{RelationGuardWork, cycle_cache_scan_slots};
use crate::types::cyclic::identity::{IDENTITY_MODE_WORK, identity_mode_bytes};
use crate::types::typevar::{TypeVarDefaultVisitor, TypeVarInstance};
use crate::types::Type;
use crate::types::local_transfer::hash_table;
use crate::types::local_transfer::collections::{
    CALL_0, CALL_1, CALL_2, CALL_3, CHECK_LANGUAGE_UB, CHECKED_MULTIPLY, LAYOUT_FROM_SIZE_ALIGNMENT,
    MAYBE_IS_ALIGNED_AND_NOT_NULL, NONNULL_AS_PTR, NONNULL_CAST, NONNULL_NEW_UNCHECKED_WORK,
    POINTER_ADD_WORK, RAW_SLICE_POINTER, SMALL_RETIRE_WRAPPERS, SMALL_TRIPLE_MUT,
    OVERFLOWING_ADD, QUOTATION_CHECKED_ARITHMETIC, QUOTATION_RESULT_TRY,
    QUOTATION_OK_OR, QUOTATION_ZIP, QUOTATION_AND_THEN, checked, prepared_smallvec_push_quote, smallvec_borrowed_quote,
    smallvec_new_quote, smallvec_metadata_quote, smallvec_pop_quote,
    smallvec_quote_preparation, smallvec_reserve_exact_quote,
    smallvec_with_capacity_quote,
};
use crate::types::visitor::{SmallSet, SmallSetLayout};
use crate::types::visitor::runtime::storage::{
    inline_scan_quote, inline_spill_quote, small_set_metadata_quote,
};

type Quote = RunResult<(usize, usize)>;
type ActiveEntry<'db> = (TypeVarInstance<'db>, OnceCell<TypeVarInstance<'db>>);

/// Matches the pinned standard library's sized, Global-allocated RcInner layout.
#[repr(C, align(2))]
#[derive(Debug)]
struct RcAllocation<T> {
    strong: Cell<usize>,
    weak: Cell<usize>,
    value: T,
}

/// Identifies the collection that the existing detector is allowed to grow at this call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardStorage {
    Active,
    Cache { capacity: Option<usize> },
}

const KEY_HASH_WORK: usize = 8 * CALL_2 + 6 * CALL_1 + 24;
const KEY_HASH_BYTES: usize = KEY_HASH_WORK * size_of::<(*mut usize, u64)>();
// TypeVarInstance -> salsa::Id -> NonZeroU32/u32, including derived wrapper calls.
const KEY_EQUAL_WORK: usize = 8 * CALL_2 + 4 * CALL_1 + 16;

/// Combines independently funded phases without converting byte widths into work.
pub(super) const fn add(left: Quote, right: Quote) -> Quote {
    match (left, right) {
        (Ok((lw, lb)), Ok((rw, rb))) => match (lw.checked_add(rw), lb.checked_add(rb)) {
            (Some(work), Some(bytes)) => Ok((work, bytes)),
            _ => Err(RunError::Contract("checked-default quotation overflow")),
        },
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

/// Quotes fixed events using the largest complete argument/result carrier for those events.
pub(super) const fn events(work: usize, widths: &[usize]) -> Quote {
    let mut width = 0;
    let mut index = 0;
    while index < widths.len() {
        if widths[index] > width {
            width = widths[index];
        }
        index += 1;
    }
    match work.checked_mul(width) {
        Some(bytes) => Ok((work, bytes)),
        None => Err(RunError::Contract("checked-default fixed quotation overflow")),
    }
}

/// Admits a synchronous quotation and checks completion before its associated operation.
pub(super) fn admit(endpoint: &TaskEndpoint<'_, '_>, quote: Quote) -> RunResult<()> {
    let (work, requested_bytes) = quote?;
    endpoint.admit_work(work)?;
    endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
    endpoint.check_completion()?;
    Ok(())
}

/// Quotes a new sized Rc and its final strong/weak release; nested owners are separate.
pub(super) const fn rc_owner<T>() -> Quote {
    // Rc and Arc share the sized Box::leak/NonNull/from_inner and final Global paths.
    // Keep that existing bound, including its atomic operations, and additionally fund
    // Rc's Cell counter and WeakInner wrappers. Nested value owners remain separate.
    const CONSTRUCT: usize = 12 * CALL_1 + 2 * CALL_2 + 32;
    const RETIRE: usize = 34 * CALL_1 + 6 * CALL_2 + 8 * CALL_3 + 96;
    add(arc_owner_quote(std::alloc::Layout::new::<RcAllocation<T>>()), add(
        events(CONSTRUCT, &[
            size_of::<RcAllocation<T>>(), size_of::<T>(), size_of::<Rc<T>>(),
            size_of::<std::mem::ManuallyDrop<Box<RcAllocation<T>>>>(),
            size_of::<(*mut RcAllocation<T>,)>(), size_of::<usize>(),
        ]),
        events(RETIRE, &[
            size_of::<Rc<T>>(), size_of::<(&Cell<usize>, usize)>(),
            size_of::<(*const (), usize, bool)>(), size_of::<std::alloc::Layout>(),
            size_of::<Option<(&Cell<usize>, &Cell<usize>)>>(),
            size_of::<usize>(), size_of::<bool>(),
        ]),
    ))
}

/// Quotes an Rc clone and ordinary strong release; the allocation quote funds final retirement.
pub(super) const fn rc_clone<T>() -> Quote {
    // Clone includes the successful assert_unchecked and overflow branch. Release reads
    // and decrements the strong Cell; the original owner's quote funds final destruction.
    const CLONE: usize = 22 * CALL_1 + 6 * CALL_2 + 3 * CALL_3 + 72;
    const RELEASE: usize = 12 * CALL_1 + 2 * CALL_2 + 24;
    events(CLONE + RELEASE, &[
        size_of::<Rc<T>>(), size_of::<(&Cell<usize>, usize)>(),
        size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
    ])
}

/// Quotes the finite arithmetic used by local owner quotations.
pub(super) const fn preparation() -> Quote {
    add(hash_table::preparation_quote(), guard_preparation())
}

/// Quotes fixed dispatch, arithmetic and composition retained by local owner quotations.
pub(super) const fn guard_preparation() -> Quote {
    // Up to 24 binary checked/composition calls, sixteen unary/result calls and eighty
    // scalar/dispatch events. Buffer and table layout preparation is additional.
    events(24 * CALL_2 + 16 * CALL_1 + 80, &[
        size_of::<RelationGuardWork>(), size_of::<GrowthPlan>(),
        size_of::<(usize, usize, usize, usize)>(), size_of::<Option<usize>>(),
        size_of::<Option<(usize, usize)>>(), size_of::<RunResult<(usize, usize)>>(),
    ])
}

/// Quotes the `guard` call selecting and returning a constant for `KeyCheck`,
/// `Candidate`, `Identity` or `ActivePush`; only `KeyCheck` inspects the storage variant.
/// Dynamic guard events use the branch quotations below.
pub(super) const fn fixed_guard_preparation() -> Quote {
    // The guard call transfers three arguments. Four events each bound event dispatch,
    // storage dispatch, constant-result construction and return forwarding.
    events(CALL_3 + 16, &[
        size_of::<(GuardStorage, RelationGuardWork, usize)>(), size_of::<Quote>(),
        size_of::<GuardStorage>(), size_of::<RelationGuardWork>(),
    ])
}

// Each four-event group covers a value read, binding, tag/construction and forwarding
// at one named scalar site. Checked arithmetic includes the native intrinsic bodies,
// `unlikely`, and its overflow-only `cold_path` call. Try includes residual conversion.
const QUOTE_CHECKED: usize = QUOTATION_CHECKED_ARITHMETIC;
const QUOTE_OPTION_TRY: usize = 2 * CALL_1 + 4 * 4;
const QUOTE_RESULT_TRY: usize = QUOTATION_RESULT_TRY;
const QUOTE_OK_OR: usize = QUOTATION_OK_OR + 4;
const QUOTE_ZIP: usize = QUOTATION_ZIP;
const QUOTE_AND_THEN: usize = QUOTATION_AND_THEN + 4;
const QUOTE_RESULT_OK: usize = CALL_1 + 3 * 4;
const QUOTE_MAP_ERR: usize = 2 * CALL_2 + 5 * 4;
const QUOTE_UNWRAP_OR: usize = CALL_2 + 3 * 4;
const QUOTE_IS_NONE: usize = 2 * CALL_1 + 2 * 4;
const QUOTE_MAP_OR: usize = CALL_3 + CALL_2 + 5 * 4;
const QUOTE_ADD: usize = CALL_2 + 2 * QUOTE_CHECKED + 8 * 4;
const QUOTE_REPEAT: usize = CALL_2 + 2 * QUOTE_CHECKED + 7 * 4;
const QUOTE_HASH_SLOTS: usize = CALL_1 + 3 * QUOTE_CHECKED
    + 2 * QUOTE_AND_THEN + QUOTE_OK_OR + 4;
const QUOTE_CACHE_SLOTS: usize = CALL_1 + QUOTE_MAP_OR + QUOTE_HASH_SLOTS + 4;
const QUOTE_INLINE_SLOTS: usize = CALL_1 + QUOTE_MAP_OR + 4;
const QUOTE_OLD_SLOTS: usize = CALL_1 + 3 * QUOTE_CHECKED
    + 2 * QUOTE_OPTION_TRY + 3 * 4;

/// Quotes runtime guard quotation, including whole callback and result carriers.
/// Fixed layout arithmetic is evaluated by const callers. Runtime arithmetic and early
/// errors remain funded before `guard` evaluates either its structural or native quote.
const fn dynamic_guard_preparation(branch_work: usize) -> Quote {
    // Common sites: guard dispatch, closure capture, closure dispatch, tuple/Option
    // construction, dynamic binding, constant loading, fixed binding and return.
    // native_guard additionally selects/binds its base, dispatches storage and the
    // second event match, and forwards the result. Its dynamic branches are additional.
    const COMMON: usize = CALL_3 + CALL_0 + QUOTE_OK_OR + QUOTE_RESULT_TRY
        + 2 * QUOTE_ADD + CALL_2 + (8 + 5) * 4;
    events(COMMON + branch_work, &[
        size_of::<(GuardStorage, RelationGuardWork, usize)>(),
        size_of::<(GuardStorage, GrowthPlan)>(),
        size_of::<(Quote, Quote)>(), size_of::<(Quote, usize)>(),
        size_of::<RunResult<Quote>>(),
        size_of::<(Option<usize>, RunError)>(),
        size_of::<(Option<(usize, usize)>, RunError)>(),
        size_of::<(Option<usize>, Option<usize>)>(),
        size_of::<(Option<usize>, (usize, usize))>(),
        size_of::<(Option<usize>, Result<usize, TddError<RunError>>)>(),
        size_of::<(Result<usize, TddError<RunError>>,)>(),
        size_of::<(usize, GrowthPlan, usize)>(),
        size_of::<ControlFlow<RunResult<Infallible>, (usize, usize)>>(),
        size_of::<ControlFlow<RunResult<Infallible>, Quote>>(),
        size_of::<ControlFlow<Result<Infallible, TddError<RunError>>, usize>>(),
        size_of::<ControlFlow<Option<Infallible>, (usize, usize)>>(),
    ])
}

// CacheAccess covers inline probes 1/3, the arbitrary-probe fallback, and spilled
// access. Only the fallback/spilled branch repeats a native quote at runtime.
pub(super) const CACHE_ACCESS_PREPARATION: Quote = dynamic_guard_preparation(
    QUOTE_CACHE_SLOTS + QUOTE_RESULT_OK + QUOTE_OPTION_TRY
        + 6 * (QUOTE_CHECKED + QUOTE_OPTION_TRY) + 2 * 4
        + CALL_1 + 3 * 4 + QUOTE_ADD + QUOTE_REPEAT,
);
pub(super) const SCAN_PREPARATION: Quote = dynamic_guard_preparation(
    3 * (QUOTE_CHECKED + QUOTE_OPTION_TRY) + 2 * QUOTE_ADD + QUOTE_REPEAT,
);
pub(super) const FINISH_PREPARATION: Quote = dynamic_guard_preparation(
    3 * (QUOTE_CHECKED + QUOTE_OPTION_TRY),
);
pub(super) const RESOURCE_PREPARATION: Quote = dynamic_guard_preparation(0);
pub(super) const INLINE_CACHE_SCAN_PREPARATION: Quote = dynamic_guard_preparation(
    QUOTE_INLINE_SLOTS + QUOTE_RESULT_OK + QUOTE_OPTION_TRY
        + 2 * (QUOTE_CHECKED + QUOTE_OPTION_TRY) + QUOTE_ADD,
);
pub(super) const SPILLED_CACHE_SCAN_PREPARATION: Quote = add(
    dynamic_guard_preparation(
        2 * QUOTE_CACHE_SLOTS + QUOTE_RESULT_OK + QUOTE_OPTION_TRY
            + 2 * (QUOTE_CHECKED + QUOTE_OPTION_TRY) + QUOTE_ADD
            + QUOTE_MAP_ERR + QUOTE_RESULT_TRY,
    ),
    hash_table::preparation_quote(),
);
pub(super) const ACTIVE_RELOCATION_PREPARATION: Quote = dynamic_guard_preparation(
    CALL_2 + QUOTE_RESULT_TRY + 6 * QUOTE_CHECKED + 4 * QUOTE_AND_THEN
        + QUOTE_ZIP + QUOTE_OK_OR + 6 * 4 + QUOTE_RESULT_OK,
);
pub(super) const CACHE_RELOCATION_PREPARATION: Quote = add(
    dynamic_guard_preparation(
        CALL_2 + 3 * QUOTE_UNWRAP_OR + QUOTE_OLD_SLOTS + QUOTE_OK_OR
            + QUOTE_RESULT_TRY + QUOTE_CHECKED + QUOTE_OK_OR + QUOTE_RESULT_TRY
            + QUOTE_CACHE_SLOTS + QUOTE_MAP_ERR + QUOTE_RESULT_TRY
            + CALL_0 + QUOTE_OK_OR + 7 * 4
            + 9 * (QUOTE_CHECKED + QUOTE_OPTION_TRY)
            + QUOTE_IS_NONE + 2 * QUOTE_ADD + 7 * 4 + QUOTE_RESULT_OK,
    ),
    hash_table::preparation_quote(),
);

/// Quotes preparation of an empty SmallVec-backed visitor or self-reference state.
pub(super) const fn owner_preparation() -> Quote {
    add(preparation(), smallvec_quote_preparation())
}

// Successful RefCell borrow/borrow_mut, one dereference and guard drop. Cell::replace
// uses intrinsic read_via_copy/write_via_move; it does not call ptr::read/ptr::write.
const REF_CELL: usize = NONNULL_NEW_UNCHECKED_WORK + NONNULL_AS_PTR
    + 24 * CALL_1 + 8 * CALL_2 + 64;
// SmallVec dereference -> triple -> slice::from_raw_parts, followed by last/index.
const ACTIVE_READ: usize = SMALL_TRIPLE_MUT + 8 * CALL_1 + 4 * CALL_2
    + RAW_SLICE_POINTER + CHECK_LANGUAGE_UB + MAYBE_IS_ALIGNED_AND_NOT_NULL
    + CHECKED_MULTIPLY + POINTER_ADD_WORK + 48;
// Slice iterator construction/advance and the callback, including the debug pointer path.
const SLICE_STEP: usize = 8 * CALL_1 + 8 * CALL_2 + 2 * POINTER_ADD_WORK
    + CHECK_LANGUAGE_UB + 64;
// OnceCell::get/get_or_init -> get_or_try_init/try_init and the successful set path.
const ONCE_CELL: usize = 16 * CALL_1 + 6 * CALL_2 + 48;
const CACHE_METADATA: usize = 8 * CALL_1 + 16;

// The inline cache uses slice::Iter over at most two non-ZST entries. Iter::new
// uses raw-pointer add; Iter::next instead uses NonNull::add's offset intrinsic.
const INLINE_ITER_NEW: usize = 5 * CALL_1 + NONNULL_CAST + NONNULL_AS_PTR
    + POINTER_ADD_WORK + 16;
const INLINE_POINTER_EQUAL: usize = CALL_2 + 2 * NONNULL_AS_PTR + 4;
const INLINE_POINTER_ADVANCE: usize = CALL_2 + NONNULL_AS_PTR + CALL_2 + CALL_1 + 8;
const INLINE_POINTER_REFERENCE: usize = CALL_1 + NONNULL_AS_PTR + CALL_1 + 4;
const INLINE_ITER_NEXT: usize = 2 * CALL_1 + INLINE_POINTER_EQUAL
    + INLINE_POINTER_ADVANCE + INLINE_POINTER_REFERENCE + 20;

// The exact scan borrows its slice once. A terminal next does not advance the
// pointer, construct an entry reference or invoke the key-comparison callback.
const EXACT_SCAN_FIXED: usize = 4 * CALL_2 + 2 * CALL_1 + INLINE_ITER_NEW + 24;
const EXACT_SCAN_TERMINAL: usize = 2 * CALL_1 + INLINE_POINTER_EQUAL + 20;
const EXACT_SCAN_YIELD: usize = INLINE_ITER_NEXT + CALL_1 + KEY_EQUAL_WORK + CALL_2 + 8;
// The admitted candidate scan maps a Range<usize>. Its terminal None skips the
// mapper, so it performs no active-buffer borrow, index or copied-entry construction.
const CANDIDATE_SCAN_TERMINAL: usize = 3 * CALL_1 + 2 * CALL_2 + 24;

/// Quotes the longest inline lookup: two comparisons and the terminal iterator check.
const fn inline_cache_lookup() -> Quote {
    const LOOKUP: usize = 2 * CALL_2 + INLINE_ITER_NEW + 3 * INLINE_ITER_NEXT
        + 2 * (CALL_1 + KEY_EQUAL_WORK + CALL_2 + 8) + 24;
    add(cell_quote(LOOKUP), events(4 * CALL_1 + CALL_2 + 12, &[
        size_of::<Option<Type<'_>>>(), size_of::<Option<Option<Type<'_>>>>(),
        size_of::<Option<&Option<Type<'_>>>>(), size_of::<(*const (), usize)>(),
    ]))
}

/// Quotes insertion into Empty or One, including owned cache moves and shallow retirement.
const fn inline_cache_insert() -> Quote {
    // Preparation spills Two before commit. The debug duplicate lookup is counted
    // separately, leaving insert_completed -> insert_new -> mem::replace here.
    events(2 * CALL_3 + CALL_2 + CALL_1 + CALL_2 + 48, &[
        TypeVarDefaultVisitor::<'_>::cache_replacement_width(),
        size_of::<(&mut (), TypeVarInstance<'_>, Option<Type<'_>>)>(),
        size_of::<[(TypeVarInstance<'_>, Option<Type<'_>>); 2]>(),
    ])
}

/// Quotes one lookup or the two lookups and insertion prepaid for an inline commit.
const fn inline_cache_access(probes: usize) -> Quote {
    match probes {
        1 => const { inline_cache_lookup() },
        3 => const { add(repeat(inline_cache_lookup(), 2), inline_cache_insert()) },
        // Other callers may supply a different count; bound either operation per access.
        _ => repeat(const { add(inline_cache_lookup(), inline_cache_insert()) }, probes),
    }
}

/// Quotes both inline `key_has_fixed_cost` checks before the first spill.
const fn inline_cache_scan() -> Quote {
    cell_quote(CALL_2 + INLINE_ITER_NEW + 3 * INLINE_ITER_NEXT
        + 2 * (2 * CALL_1 + 8) + 24)
}

/// Quotes the selected sized RefCell wrappers and their complete borrowed carriers.
const fn cell_quote(work: usize) -> Quote {
    events(work, &[
        size_of::<std::cell::Ref<'_, usize>>(), size_of::<std::cell::RefMut<'_, usize>>(),
        size_of::<Result<std::cell::Ref<'_, usize>, std::cell::BorrowError>>(),
        size_of::<Result<std::cell::RefMut<'_, usize>, std::cell::BorrowMutError>>(),
        size_of::<(&Cell<isize>, isize)>(), size_of::<(*const (), usize, bool)>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(&[usize], usize)>(),
        size_of::<Option<&usize>>(), size_of::<usize>(),
    ])
}

/// Quotes one mutable set borrow and its eventual borrow-guard release.
pub(super) const fn variable_borrow() -> Quote {
    cell_quote(REF_CELL)
}

/// Quotes constructing a snapshot without comparing its fixed key.
const fn native_snapshot_read() -> Quote {
    add(cell_quote(2 * REF_CELL + ACTIVE_READ + CACHE_METADATA
        + 12 * CALL_2 + 32),
        smallvec_metadata_quote::<ActiveEntry<'_>, 6>())
}

/// Quotes snapshot construction and one fixed-key comparison.
const fn native_snapshot() -> Quote {
    add(native_snapshot_read(), cell_quote(KEY_EQUAL_WORK))
}

/// Quotes commit's snapshot comparison, item check and cache borrows.
/// The caller admits this once before finish preparation.
pub(super) const fn finish_commit() -> Quote {
    // The item check calls Option::ne -> Option::eq before fixed-key equality.
    add(native_snapshot(), cell_quote(2 * REF_CELL + KEY_EQUAL_WORK + 2 * CALL_2 + 16))
}

const fn repeat(quote: Quote, count: usize) -> Quote {
    match quote {
        Ok((work, bytes)) => match (work.checked_mul(count), bytes.checked_mul(count)) {
            (Some(work), Some(bytes)) => Ok((work, bytes)),
            _ => Err(RunError::Contract("checked-default repeated quotation overflow")),
        },
        Err(error) => Err(error),
    }
}

// These native operations remain charged when their quotations are selected at runtime.
// Keep KeyCheck's storage variants and Candidate/Identity's different operations distinct.
const KEY_CHECK_ACTIVE_NATIVE: Quote = add(
    native_snapshot_read(),
    cell_quote(2 * REF_CELL + ACTIVE_READ + CACHE_METADATA),
);
const KEY_CHECK_CACHE_NATIVE: Quote = add(
    native_snapshot(), cell_quote(REF_CELL + ACTIVE_READ),
);
const CANDIDATE_NATIVE: Quote = add(
    repeat(native_snapshot(), 2), cell_quote(2 * (REF_CELL + ACTIVE_READ + ONCE_CELL)),
);
const IDENTITY_NATIVE: Quote = add(
    repeat(native_snapshot(), 2), cell_quote(REF_CELL + ACTIVE_READ + ONCE_CELL),
);
const ACTIVE_PUSH_NATIVE: Quote = add(
    add(native_snapshot(), cell_quote(3 * REF_CELL + ACTIVE_READ + ONCE_CELL)),
    add(
        prepared_smallvec_push_quote::<ActiveEntry<'_>, 6>(),
        smallvec_pop_quote::<ActiveEntry<'_>, 6>(),
    ),
);

/// Adds native wrappers below the detector's separately counted structural decisions.
fn native_guard<'db>(storage: GuardStorage, event: RelationGuardWork) -> Quote {
    let quote = match event {
        // Lookup reads the cache layout before its first CacheAccess admission. Finish
        // instead compares the initial snapshot's top key before its first Finish admission.
        RelationGuardWork::KeyCheck => match storage {
            GuardStorage::Active => KEY_CHECK_ACTIVE_NATIVE,
            GuardStorage::Cache { .. } => KEY_CHECK_CACHE_NATIVE,
        },
        // Only lookup can enter an active scan after a cache miss. The miss also reads
        // the active length before ExactScan. Finish only validates the saved snapshot.
        RelationGuardWork::CacheAccess { .. } => match storage {
            GuardStorage::Active => const { add(add(add(native_snapshot(), native_snapshot_read()), cell_quote(2 * REF_CELL + CACHE_METADATA)), smallvec_metadata_quote::<ActiveEntry<'db>, 6>()) },
            GuardStorage::Cache { .. } => const { add(native_snapshot(), cell_quote(2 * REF_CELL + CACHE_METADATA)) },
        },
        RelationGuardWork::ExactScan { .. } => const {
            add(
                add(native_snapshot(), cell_quote(REF_CELL + ACTIVE_READ)),
                Ok((IDENTITY_MODE_WORK, identity_mode_bytes::<TypeVarInstance<'db>>())),
            )
        },
        RelationGuardWork::CandidateScan { .. } => const { add(native_snapshot(), cell_quote(8 * CALL_1 + 24)) },
        RelationGuardWork::Candidate => CANDIDATE_NATIVE,
        RelationGuardWork::Identity => IDENTITY_NATIVE,
        // The third borrow and pop are prepaid for completion or cancellation.
        RelationGuardWork::ActivePush => ACTIVE_PUSH_NATIVE,
        // The outer preparation callback owns commit's fixed native quote. Each Finish
        // retains validation and the cache-layout read following the first event.
        RelationGuardWork::Finish => const { add(native_snapshot(), cell_quote(REF_CELL + CACHE_METADATA)) },
        RelationGuardWork::CacheKeyScan { .. } => const { add(native_snapshot(), cell_quote(REF_CELL)) },
        RelationGuardWork::Relocate { .. } => const { native_snapshot() },
        // Only cache growth refreshes its snapshot after mutation. Active growth proceeds
        // directly to the already-funded push and retains no refreshed snapshot.
        RelationGuardWork::Resource { .. } => match storage {
            GuardStorage::Active => const { add(native_snapshot(), cell_quote(REF_CELL)) },
            GuardStorage::Cache { .. } => const { add(add(native_snapshot(), native_snapshot_read()), cell_quote(REF_CELL)) },
        },
    };
    match event {
        RelationGuardWork::ExactScan { len } => add(quote,
            add(const { cell_quote(EXACT_SCAN_FIXED + EXACT_SCAN_TERMINAL) },
                repeat(const { cell_quote(EXACT_SCAN_YIELD) }, len))),
        RelationGuardWork::CandidateScan { len } => add(quote,
            add(const { cell_quote(CANDIDATE_SCAN_TERMINAL) },
                repeat(const { cell_quote(REF_CELL + ACTIVE_READ + SLICE_STEP) }, len))),
        RelationGuardWork::CacheAccess { capacity: None, probes } => add(quote,
            inline_cache_access(probes)),
        RelationGuardWork::CacheAccess { capacity: Some(_), probes } => add(quote,
            repeat(const { hash_table::access_quote::<TypeVarInstance<'db>, Option<Type<'db>>>() }, probes)),
        RelationGuardWork::CacheKeyScan { capacity: None } => add(quote,
            const { inline_cache_scan() }),
        RelationGuardWork::CacheKeyScan { capacity: Some(capacity) } => add(quote,
            hash_table::scan_quote::<TypeVarInstance<'db>, Option<Type<'db>>>(
                cycle_cache_scan_slots::<RunError>(Some(capacity)).map_err(|_| RunError::Contract("default cache scan overflow"))?)),
        RelationGuardWork::Candidate | RelationGuardWork::ActivePush
        | RelationGuardWork::Identity | RelationGuardWork::Finish | RelationGuardWork::KeyCheck
        | RelationGuardWork::Relocate { .. }
        | RelationGuardWork::Resource { .. } => quote,
    }
}

/// Quotes advancing one borrowed self-reference type cursor, including its terminal check.
pub(super) const fn type_cursor<'db>() -> Quote {
    events(SLICE_STEP + 16, &[
        size_of::<(&[Type<'db>], usize)>(), size_of::<Option<Type<'db>>>(),
        size_of::<Type<'db>>(), size_of::<RunResult<Option<Type<'db>>>>(),
        size_of::<(*const (), usize, usize)>(), size_of::<(*const (), usize, bool)>(),
    ])
}

/// Quotes constructing the six-inline-entry default visitor and its empty owners.
pub(super) fn visitor<'db>() -> Quote {
    if size_of::<ActiveEntry<'db>>() != TypeVarDefaultVisitor::admitted_active_entry_bytes() {
        return Err(RunError::Contract("default active entry representation changed"));
    }
    add(const { rc_owner::<TypeVarDefaultVisitor<'db>>() }, add(
        smallvec_with_capacity_quote::<ActiveEntry<'db>, 6>(6),
        const { events(32, &[
            size_of::<TypeVarDefaultVisitor<'db>>(), size_of::<Option<Type<'db>>>(),
            size_of::<usize>(),
        ]) },
    ))
}

/// Combines a fixed guard event's structural, snapshot, carrier and native quotations.
/// Callers evaluate this in const blocks; the returned work and bytes still fund the guard.
const fn fixed_guard_quote<'db>(work: usize, snapshots: usize, native: Quote) -> Quote {
    let snapshot = match events(48, &[
        size_of::<TypeVarInstance<'db>>(), size_of::<Option<Type<'db>>>(),
        size_of::<(usize, usize, usize, usize)>(), size_of::<bool>(),
    ]) {
        Ok((snapshot_work, snapshot_bytes)) => {
            match (snapshot_work.checked_mul(snapshots), snapshot_bytes.checked_mul(snapshots)) {
                (Some(work), Some(bytes)) => Ok((work, bytes)),
                _ => Err(RunError::Contract("TypeVar default snapshot quotation overflow")),
            }
        }
        Err(error) => return Err(error),
    };
    let carriers = Ok((0,
        TypeVarDefaultVisitor::admitted_transient_bytes() * (snapshots + 1)
            + size_of::<TypeVarInstance<'db>>() * 8 + size_of::<Option<Type<'db>>>() * 4));
    add(add(add(Ok((work, 0)), snapshot), carriers), native)
}

/// Quotes a guard step plus its snapshot validation and concrete transient carriers.
pub(super) fn guard<'db>(
    storage: GuardStorage,
    event: RelationGuardWork,
    result_payload: usize,
) -> Quote {
    const KEY_WORK: usize = 4;
    match event {
        RelationGuardWork::KeyCheck => return match storage {
            GuardStorage::Active => const { fixed_guard_quote(16, 0, KEY_CHECK_ACTIVE_NATIVE) },
            GuardStorage::Cache { .. } => const { fixed_guard_quote(16, 0, KEY_CHECK_CACHE_NATIVE) },
        },
        RelationGuardWork::Candidate => return const { fixed_guard_quote(16, 2, CANDIDATE_NATIVE) },
        RelationGuardWork::Identity => return const { fixed_guard_quote(16, 2, IDENTITY_NATIVE) },
        RelationGuardWork::ActivePush => return const { fixed_guard_quote(48, 1, ACTIVE_PUSH_NATIVE) },
        RelationGuardWork::CacheAccess { .. } | RelationGuardWork::ExactScan { .. }
        | RelationGuardWork::CandidateScan { .. }
        | RelationGuardWork::Finish | RelationGuardWork::CacheKeyScan { .. }
        | RelationGuardWork::Relocate { .. } | RelationGuardWork::Resource { .. } => {},
    };
    let dynamic = (|| -> Option<(usize, usize)> {
        match event {
            RelationGuardWork::CacheAccess { capacity, probes } => {
                let slots = cycle_cache_scan_slots::<RunError>(capacity).ok()?;
                let (hash_work, hash_bytes) = match capacity {
                    None => (0, 0),
                    Some(_) => (KEY_HASH_WORK, KEY_HASH_BYTES),
                };
                Some((slots.checked_mul(const { KEY_WORK + 2 })?.checked_add(hash_work)?.checked_mul(probes)?,
                    slots.checked_mul(const { size_of::<TypeVarInstance<'db>>() * 2 })?.checked_add(hash_bytes)?.checked_mul(probes)?))
            }
            RelationGuardWork::ExactScan { len } => Some((len.checked_mul(const { KEY_WORK + 2 })?.checked_add(8)?,
                len.checked_mul(const { size_of::<TypeVarInstance<'db>>() * 2 })?)),
            RelationGuardWork::CandidateScan { len } => Some((len.checked_mul(12)?.checked_add(8)?,
                len.checked_mul(const { TypeVarDefaultVisitor::admitted_scan_transfer_bytes() })?)),
            RelationGuardWork::Candidate => Some((16, 0)),
            RelationGuardWork::Identity => Some((16, 0)),
            RelationGuardWork::KeyCheck => Some((16, 0)),
            // Insertion prepays removing the active entry if its suspended parent is dropped.
            RelationGuardWork::ActivePush => Some((48, 0)),
            // Cached visits return without recomputation, so finish compares only the incoming
            // result with its prepared copy. No existing cached result needs a payload quote.
            RelationGuardWork::Finish => Some((result_payload.checked_mul(2)?.checked_add(128)?, result_payload.checked_mul(2)?)),
            RelationGuardWork::CacheKeyScan { capacity } => {
                let slots = cycle_cache_scan_slots::<RunError>(capacity).ok()?;
                Some((slots.checked_mul(4)?, slots.checked_mul(const { size_of::<TypeVarInstance<'db>>() })?))
            }
            RelationGuardWork::Relocate { plan } => relocation::<TypeVarInstance<'db>, Option<Type<'db>>>(storage, plan).ok(),
            RelationGuardWork::Resource { requested_bytes } => Some((0, requested_bytes)),
        }
    })().ok_or(RunError::Contract("TypeVar default visitor quotation overflow"));
    // An events() error returns before the final combination. An error adding the
    // fixed carriers remains a Quote, so a structural `dynamic` error wins. Native
    // quotation still runs before the final add chooses its leftmost error.
    let fixed = const {
        match events(48, &[
            size_of::<TypeVarInstance<'db>>(), size_of::<Option<Type<'db>>>(),
            size_of::<(usize, usize, usize, usize)>(), size_of::<bool>(),
        ]) {
            Ok(snapshot) => Ok(add(Ok(snapshot), Ok((0,
                TypeVarDefaultVisitor::admitted_transient_bytes() * 2
                    + size_of::<TypeVarInstance<'db>>() * 8
                    + size_of::<Option<Type<'db>>>() * 4)))),
            Err(error) => Err(error),
        }
    }?;
    add(add(dynamic, fixed), native_guard(storage, event))
}

/// Quotes new storage, relocation and eventual retirement for one existing detector growth plan.
fn relocation<K: Copy, V: Copy>(storage: GuardStorage, plan: GrowthPlan) -> Quote {
    match storage {
        GuardStorage::Active => {
            let buffer = if plan.relocation_units > 6 {
                const { smallvec_reserve_exact_quote::<ActiveEntry<'static>, 6>(true, true) }
            } else {
                const { smallvec_reserve_exact_quote::<ActiveEntry<'static>, 6>(true, false) }
            }?;
            let work = plan.relocation_units.checked_mul(2)
                .and_then(|work| work.checked_add(plan.requested_capacity))
                .and_then(|work| work.checked_add(buffer.0));
            let bytes = plan.relocation_units.checked_mul(const { TypeVarDefaultVisitor::admitted_active_entry_bytes() })
                .and_then(|bytes| bytes.checked_mul(2))
                .and_then(|bytes| bytes.checked_add(buffer.1));
            work.zip(bytes).ok_or(RunError::Contract("default active storage quotation overflow"))
        }
        GuardStorage::Cache { capacity } => {
            let len = capacity.unwrap_or(2);
            let old_slots = slots(capacity.unwrap_or(0))
                .ok_or(RunError::Contract("default old cache slots overflow"))?;
            let slots = cycle_cache_scan_slots::<RunError>(Some(plan.requested_capacity.checked_mul(2)
                .ok_or(RunError::Contract("default cache capacity overflow"))?))
                .map_err(|_| RunError::Contract("default cache slots overflow"))?;
            let quote = (|| -> Option<(usize, usize)> {
                let hashes = len.checked_mul(const { KEY_HASH_WORK + 1 })?;
                let work = plan.relocation_units.checked_mul(2)?.checked_add(hashes)?.checked_add(32)?;
                let entry = const { size_of::<(K, V)>() };
                let bytes = slots.checked_mul(entry.checked_add(1)?)?
                    .checked_sub(plan.requested_payload_bytes)?
                    .checked_add(len.checked_mul(KEY_HASH_BYTES)?)?;
                Some((work, bytes))
            })().ok_or(RunError::Contract("default cache storage quotation overflow"));
            // CacheKeyScan supplied the previous capacity. Spilled growth occurs only
            // when full; the inline first spill contains exactly two completed entries.
            let storage = hash_table::growth_quote::<K, V>(old_slots, slots, capacity.unwrap_or(0));
            let storage = if capacity.is_none() {
                add(storage, const { add(hash_table::empty_quote::<K, V>(),
                    repeat(hash_table::access_quote::<K, V>(), 2)) })
            } else {
                storage
            };
            add(quote, storage)
        }
    }
}

/// Quotes the fixed-key adapter call; equality and hashing are paid by `variable_set`.
pub(super) const fn variable_key_access<'db>() -> Quote {
    events(2 * CALL_2 + CALL_1 + 12, &[
        size_of::<TypeVarInstance<'db>>(), size_of::<RunResult<()>>(),
        size_of::<Result<(), TddError<RunError>>>(),
        size_of::<&TaskEndpoint<'_, '_>>(),
    ])
}

/// Quotes the growth adapter and its payload; native relocation is paid by `variable_set`.
pub(super) const fn variable_growth(plan: GrowthPlan) -> Quote {
    add(const { events(2 * CALL_2 + CALL_1 + 12, &[
        size_of::<GrowthPlan>(), size_of::<RunResult<()>>(),
        size_of::<Result<(), TddError<RunError>>>(),
        size_of::<&TaskEndpoint<'_, '_>>(),
    ]) }, Ok((0, plan.requested_payload_bytes)))
}

/// Quotes reading the set layout and constructing its borrowed admission adapter.
pub(super) const fn variable_metadata<'db>() -> Quote {
    add(small_set_metadata_quote::<TypeVarInstance<'db>, 8>(), events(3 * CALL_1 + 12, &[
        size_of::<SmallSetLayout>(), size_of::<&TaskEndpoint<'_, '_>>(),
    ]))
}

/// Quotes fixed-size key hashing and equality independently of their transfer widths.
const fn variable_key_bodies<'db>(hashes: usize, comparisons: usize) -> Quote {
    add(repeat(Ok((KEY_HASH_WORK + 1, KEY_HASH_BYTES)), hashes),
        repeat(events(KEY_EQUAL_WORK, &[
            size_of::<(&TypeVarInstance<'db>, &TypeVarInstance<'db>)>(), size_of::<bool>(),
        ]), comparisons))
}

/// Quotes one shared insertion before key access or mutation, including a possible duplicate.
/// `variable_growth` supplies the requested payload once; `hash_table::growth_quote` supplies
/// relocation and retirement. This quote supplies hash backing beyond that requested payload.
pub(super) fn variable_set<'db>(layout: SmallSetLayout) -> Quote {
    let SmallSetLayout { inline, len, capacity } = layout;
    let overflow = || RunError::Contract("self-reference small-set quotation overflow");
    let flow = const { events(8 * CALL_1 + 4 * CALL_2 + 32, &[
        size_of::<&mut SmallSet<TypeVarInstance<'db>, 8>>(),
        size_of::<TypeVarInstance<'db>>(), size_of::<RunResult<bool>>(),
        size_of::<Result<bool, TddError<RunError>>>(),
    ]) };
    let native = if inline {
        add(inline_scan_quote::<TypeVarInstance<'db>>(len),
            variable_key_bodies(0, len))
    } else {
        add(const { add(hash_table::lookup_quote::<TypeVarInstance<'db>, ()>(),
            hash_table::prepared_insert_quote::<TypeVarInstance<'db>, ()>()) },
            variable_key_bodies(2, len.checked_mul(2).ok_or_else(overflow)?))
    };
    if len < capacity {
        let push = if inline {
            const { prepared_smallvec_push_quote::<TypeVarInstance<'db>, 8>() }
        } else {
            Ok((0, 0))
        };
        return add(flow, add(native, push));
    }
    let required = len.checked_add(1).ok_or_else(overflow)?;
    let plan = if inline {
        GrowthPlan {
            requested_capacity: required,
            requested_payload_bytes: required.checked_mul(size_of::<TypeVarInstance<'db>>()).ok_or_else(overflow)?,
            relocation_units: len,
        }
    } else {
        sequence_growth::<TypeVarInstance<'db>, RunError>(capacity, required).map_err(|_| overflow())?
    };
    let maximum_capacity = plan.requested_capacity.checked_mul(2).ok_or_else(overflow)?;
    let new_slots = hash_slots::<RunError>(maximum_capacity).map_err(|_| overflow())?;
    let old_slots = if inline { 0 } else {
        hash_slots::<RunError>(capacity).map_err(|_| overflow())?
    };
    let backing = new_slots.checked_mul(size_of::<TypeVarInstance<'db>>() + 1).ok_or_else(overflow)?;
    Layout::from_size_align(backing, align_of::<TypeVarInstance<'db>>()).map_err(|_| overflow())?;
    let supplement = backing.checked_sub(plan.requested_payload_bytes).ok_or_else(overflow)?;
    let growth = hash_table::growth_quote::<TypeVarInstance<'db>, ()>(old_slots, new_slots, if inline { 0 } else { len });
    let retained = if inline {
        // The first table receives len + 1 keys. At most 0 + ... + len existing
        // keys can reach equality during those insertions, even if every hash collides.
        let comparisons = len.checked_mul(required).ok_or_else(overflow)? / 2;
        add(const { inline_spill_quote::<TypeVarInstance<'db>, 8>() },
            variable_key_bodies(required, comparisons))
    } else {
        // The shared algorithm visits retained keys before reserve; native growth
        // separately scans buckets and rehashes each retained key without equality.
        add(hash_table::scan_quote::<TypeVarInstance<'db>, ()>(old_slots),
            variable_key_bodies(len, 0))
    };
    // The shared algorithm computes its own growth plan after the retained-key admissions.
    let planning = const { events(2 * CHECKED_MULTIPLY + 2 * OVERFLOWING_ADD
        + 12 * CALL_2 + 8 * CALL_1 + 64, &[
        size_of::<GrowthPlan>(), size_of::<Option<usize>>(), size_of::<RunResult<bool>>(),
        size_of::<Result<bool, TddError<RunError>>>(),
    ]) };
    let representation = const { events(2 * CALL_1 + 8, &[
        size_of::<SmallSet<TypeVarInstance<'db>, 8>>(),
    ]) };
    add(flow, add(native, add(planning, add(representation,
        add(growth, add(retained, Ok((0, supplement))))))))
}

/// Quotes the empty inline variable set and its final header retirement.
pub(super) const fn empty_variable_set<'db>() -> Quote {
    add(add(checked(smallvec_new_quote::<TypeVarInstance<'db>, 8>()),
        checked(smallvec_borrowed_quote::<TypeVarInstance<'db>>(SMALL_RETIRE_WRAPPERS))),
        events(CALL_1 + 4, &[size_of::<SmallSet<TypeVarInstance<'db>, 8>>()]))
}

/// Quotes evaluating `variable_set` after metadata, before inspecting any key.
pub(super) const fn variable_set_preparation() -> Quote {
    // Inline scan scaling, key-body repeats and quote aggregation use checked scalar arithmetic.
    events(12 * CHECKED_MULTIPLY + 18 * OVERFLOWING_ADD
        + 40 * CALL_2 + 30 * CALL_1 + 128, &[
        size_of::<SmallSetLayout>(), size_of::<Option<usize>>(), size_of::<Quote>(),
    ])
}

/// Quotes the additional layout and native-quotation preparation when the set is full.
pub(super) const fn variable_growth_preparation() -> Quote {
    add(repeat(hash_table::preparation_quote(), 2), events(
        8 * CHECKED_MULTIPLY + 12 * OVERFLOWING_ADD + LAYOUT_FROM_SIZE_ALIGNMENT
            + 24 * CALL_2 + 16 * CALL_1 + 64,
        &[size_of::<GrowthPlan>(), size_of::<Layout>(), size_of::<Quote>(),
            size_of::<Result<Layout, std::alloc::LayoutError>>()],
    ))
}
