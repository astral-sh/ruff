//! Storage admission for the base-variable collector's ordered, handle-only result set.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

use super::super::super::lint_diagnostic_cost::{
    BufferQuotePreparation, buffer_quote_preparation, empty_vec_quote, prepared_vec_push_quote,
    vec_reserve_exact_quote,
};
use super::super::super::return_locations::ordered_insert_quote;
use super::super::super::storage::{StorageQuote, slots, table_merge, table_merge_preparation_quote};
use crate::types::class::base_typevars::BaseTypeVarCollector;
use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, CALL_3, CALL_4, CHECKED_MULTIPLY, LAYOUT_FROM_SIZE_ALIGNMENT,
    OVERFLOWING_ADD, POINTER_ACCESS, POINTER_ADD_WORK, VECTOR_CAPACITY, VECTOR_LENGTH,
    add_quotes, checked, event_quote, fixed_quote,
};
use crate::types::local_transfer::hash_table;
use crate::types::visitor::runtime::storage::search_initial_state_quote;
use crate::types::{BoundTypeVarInstance, StaticClassLiteral};
use crate::{FxIndexSet, ProgramEnvironment};

type Entry<'db> = (usize, BoundTypeVarInstance<'db>);

// IndexSet -> IndexMap -> Core; Core::len also evaluates its debug length assertion.
const LENGTH: usize = 3 * CALL_1 + VECTOR_LENGTH + 2 * (2 * CALL_1 + 1) + 24;
const CAPACITY: usize = 3 * CALL_1 + 2 * CALL_1 + 8 + VECTOR_CAPACITY + 2 * CALL_2 + 8;
// The cached-hash and equality callbacks index the borrowed entry slice before reading a field.
const ENTRY_INDEX: usize = 4 * CALL_2 + 2 * CALL_1 + POINTER_ADD_WORK + 16;
// BuildHasherDefault/FxHasher construction, reference/derived Hash, Id::as_bits, write_u64,
// wrapping arithmetic and finish. Equality compares the Id's index and generation fields.
const KEY_HASH: usize = 10 * CALL_2 + 10 * CALL_1 + 4 * 2 + 48;
const KEY_EQUAL: usize = 8 * CALL_2 + 6 * CALL_1 + 24;

/// Quotes the empty walk state, ordered result and program environment, including header disposal.
/// Insertions and the shared walker separately fund backing storage and its eventual retirement.
pub(super) const fn initial_quote() -> RunResult<(usize, usize)> {
    let walk = match search_initial_state_quote() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    let indices = match hash_table::empty_quote::<usize, ()>() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    let entries = match empty_vec_quote::<Entry<'_>>() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    // IndexSet/IndexMap defaults, the zero-capacity branch, Core aggregation and
    // BuildHasherDefault construction surround the separately quoted empty containers.
    let ordered = fixed_quote(6 * 2 + CALL_2 + 3 * CALL_1 + 24, [
        (12, size_of::<FxIndexSet<BoundTypeVarInstance<'_>>>()),
        (8, size_of::<usize>()),
    ]);
    // ProgramEnvironment::from_program constructs a ProgramSource and Cell/UnsafeCell;
    // the collector then owns that environment and both already quoted walk containers.
    let collector = fixed_quote(6 * CALL_1 + CALL_2 + 24, [
        (16, size_of::<ProgramEnvironment<'_>>()),
        (8, size_of::<salsa::Id>()),
        (3, size_of::<BaseTypeVarCollector<'_>>()),
        (3, size_of::<StaticClassLiteral<'_>>()),
    ]);
    checked(add_quotes(add_quotes(walk, add_quotes(indices, entries)), add_quotes(ordered, collector)))
}

/// Quotes metadata reads and construction of an insertion quote before either is evaluated.
/// This includes the full-table path even when the next insertion finds an existing variable.
pub(super) const fn preparation_quote() -> RunResult<(usize, usize)> {
    let table = match table_merge_preparation_quote() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    let hash = match hash_table::preparation_quote() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    let vector = match buffer_quote_preparation(BufferQuotePreparation::VecReserveExact) {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    // ordered_insert_quote calls table_merge twice on growth; this supplement calls it once.
    // Native growth can prepare a bucket scan, and reserve_entries can try two vector reserves.
    let helpers = add_quotes(add_quotes(table, add_quotes(table, table)),
        add_quotes(add_quotes(hash, hash), add_quotes(vector, vector)));
    let local = event_quote(
        LENGTH + CAPACITY + 12 * CHECKED_MULTIPLY + 30 * OVERFLOWING_ADD
            + LAYOUT_FROM_SIZE_ALIGNMENT + 56 * CALL_2 + 48 * CALL_1 + 192,
        &[
            size_of::<(usize, usize, usize, usize)>(), size_of::<Option<usize>>(),
            size_of::<StorageQuote>(), size_of::<Option<(StorageQuote, usize)>>(),
            size_of::<RunResult<(usize, usize)>>(),
            size_of::<Option<(usize, usize)>>(),
            size_of::<Result<Layout, std::alloc::LayoutError>>(),
            size_of::<&FxIndexSet<BoundTypeVarInstance<'_>>>(),
        ],
    );
    checked(add_quotes(helpers, local))
}

/// Quotes one ordered insertion, including duplicates, native wrappers and abandoned-result cleanup.
/// The caller admits this complete quote before calling `IndexSet::insert`.
pub(super) fn insert_quote(len: usize, capacity: usize) -> RunResult<(usize, usize)> {
    let overflow = || RunError::Contract("base-variable insertion quotation overflow");
    if len > capacity {
        return Err(RunError::Contract("base-variable set length exceeds capacity"));
    }
    let payload = ordered_insert_quote(len, capacity).ok_or_else(overflow)?;
    let native = const { hash_table::prepared_insert_quote::<usize, ()>() }?;
    // HashTable::entry and RawTable::find_or_find_insert_index each pass three complete
    // argument groups. Their equality closure owns a key reference and an entry slice;
    // their hasher closure owns another slice. The generic native quote uses thin closures.
    let callbacks = const { fixed_quote(0, [(6, size_of::<(
        &mut (), u64, (&BoundTypeVarInstance<'_>, &[Entry<'_>]), &[Entry<'_>],
    )>())]) };
    let push = const { prepared_vec_push_quote::<Entry<'_>>() }?;
    let key = const { event_quote(KEY_HASH, &[
        size_of::<(&BoundTypeVarInstance<'_>, &mut usize)>(),
        size_of::<(salsa::Id, u64)>(), size_of::<u64>(),
    ]) };
    // A logical table access still has to execute each reached key's equality body.
    // In the worst case all retained keys match the candidate's hash tag.
    let equality = const { checked(event_quote(ENTRY_INDEX + 2 * CALL_2 + KEY_EQUAL + 12, &[
        size_of::<(&BoundTypeVarInstance<'_>, &[Entry<'_>], &usize)>(),
        size_of::<(&BoundTypeVarInstance<'_>, &BoundTypeVarInstance<'_>)>(),
        size_of::<usize>(), size_of::<bool>(),
    ])) }?;
    let equality = equality.0.checked_mul(len).zip(equality.1.checked_mul(len))
        .ok_or_else(overflow)?;
    let wrappers = const { event_quote(
        6 * CALL_2 + 3 * CALL_3 + CALL_4 + 8 * CALL_1 + 2 * VECTOR_LENGTH
            + VECTOR_CAPACITY + 2 * POINTER_ACCESS + 72,
        &[
            size_of::<(&mut FxIndexSet<BoundTypeVarInstance<'_>>, BoundTypeVarInstance<'_>)>(),
            size_of::<(&mut (), usize, BoundTypeVarInstance<'_>, ())>(),
            size_of::<(&BoundTypeVarInstance<'_>, &[Entry<'_>])>(),
            size_of::<(usize, Option<()>)>(), size_of::<Entry<'_>>(),
            // Private occupied entries contain a bucket and table pointer. Vacant entries
            // contain a tag, index and table pointer; the enum adds its discriminant.
            size_of::<(*mut usize, *mut ())>(),
            size_of::<(usize, u8, usize, *mut ())>(),
            size_of::<((u8, usize, *mut ()), usize)>(),
        ],
    ) };
    let fixed = add_quotes(add_quotes(add_quotes(Some(native), callbacks), Some(push)),
        add_quotes(key, add_quotes(Some(equality), wrappers)));
    let quote = add_quotes(Some((payload.work, payload.bytes)), fixed);
    if len < capacity {
        return checked(quote);
    }

    // HashTable::entry reserves before testing equality, so a duplicate can also resize.
    let old_slots = slots(capacity).ok_or_else(overflow)?;
    let (_, new_slots) = table_merge::<usize>(len, capacity, 1, 0).ok_or_else(overflow)?;
    let hash = hash_table::growth_quote::<usize, ()>(old_slots, new_slots, len)?;
    let cached = const { checked(event_quote(ENTRY_INDEX + CALL_1 + 9, &[
        size_of::<(&[Entry<'_>], &usize)>(), size_of::<u64>(),
    ])) }?;
    let cached = cached.0.checked_add(1).and_then(|work| work.checked_mul(len))
        .zip(cached.1.checked_mul(len)).ok_or_else(overflow)?;

    // reserve_entries rounds its request up to the index table's capacity. At capacity 3,
    // for example, it can request 7 entries rather than sequence_merge's bound of 6.
    let required = len.checked_add(1).ok_or_else(overflow)?;
    let sequence_capacity = capacity.checked_mul(2).ok_or_else(overflow)?.max(required).max(4);
    let entries_capacity = sequence_capacity.checked_mul(2).ok_or_else(overflow)?;
    let additional = entries_capacity.checked_sub(len).ok_or_else(overflow)?;
    let reserve = vec_reserve_exact_quote::<Entry<'_>>(len, capacity, additional)?;
    // The first fallible reservation may fail, after which indexmap retries with one extra slot.
    let fallback = vec_reserve_exact_quote::<Entry<'_>>(len, capacity, 1)?;
    let already_paid = sequence_capacity.checked_add(len)
        .and_then(|entries| entries.checked_mul(size_of::<Entry<'_>>()))
        .ok_or_else(overflow)?;
    let reserve = (reserve.0, reserve.1.checked_sub(already_paid).ok_or_else(overflow)?);
    checked(add_quotes(quote, add_quotes(add_quotes(Some(hash), Some(cached)),
        add_quotes(Some(reserve), Some(fallback)))))
}
