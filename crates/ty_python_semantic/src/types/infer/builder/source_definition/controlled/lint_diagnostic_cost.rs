//! Fixed quotations for the owned diagnostic paths used by controlled lint reporting.

use std::alloc::Layout;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

use ruff_db::diagnostic::{Annotation, DiagnosticMessage, SubDiagnostic, SubDiagnosticSeverity};
use salsa::execution_probe::{RunError, RunResult};

use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, CALL_3, CALL_5, NONNULL_NEW_UNCHECKED_WORK, POINTER_ACCESS, POINTER_READ, POINTER_WRITE, POINTER_ADD_WORK, MANUALLY_DROP, MANUALLY_DROP_DEREF, VECTOR_CAPACITY, VECTOR_LENGTH, RAW_SLICE_POINTER, BOX_FROM_RAW, BOX_INTO_RAW, BOX_ALLOCATION_WORK, BOX_RETIREMENT_WORK, EMPTY_VECTOR_CONSTRUCTION, EMPTY_VECTOR_RETIREMENT, FixedQuote, fixed_quote, event_quote, add_quotes,
};

mod buffers;
mod context;

pub(in crate::types::infer::builder) use buffers::{
    BufferQuotePreparation, annotation_with_ty_span_quote, buffer_quote_preparation, diagnostic_with_capacity_quote,
    empty_vec_quote, prepared_vec_push_quote, string_push_str_quote, string_with_capacity_quote,
    arc_owner_quote, fixed_box_quote, single_inline_smallvec_quote, vec_into_boxed_slice_quote, vec_reserve_exact_quote, vector_with_capacity_quote,
    prepared_smallvec_push_quote, smallvec_with_capacity_quote, vec_into_smallvec_quote,
    name_clone_retirement_quote, smallvec_metadata_quote,
};
pub(in crate::types::infer::builder) use context::{
    ReportingMetadataOperation, reporting_metadata_quote,
    context_mutation_quote, context_reservation_preparation_quote, context_storage_quote, lint_metadata_quote, lint_text_parts_quote,
    suppression_storage_preparation_quote,
};

const UNIQUE_MUTATION_WORK: usize = 3 * 64 + 2 * 47 + 103;
// String and Vec entry, no-shrink test, ManuallyDrop, buffer and length extraction.
const MESSAGE_ENTRY_WORK: usize = 3 * CALL_1 + VECTOR_CAPACITY + MANUALLY_DROP + POINTER_READ
    + 2 * MANUALLY_DROP_DEREF + VECTOR_LENGTH + 11;
// RawVec::into_box: capacity assertion, pointer construction, allocator read and boxing.
const MESSAGE_RAW_BUFFER_WORK: usize = CALL_2 + VECTOR_CAPACITY + MANUALLY_DROP + 2 * MANUALLY_DROP_DEREF
    + POINTER_ACCESS + CALL_1 + 2 + RAW_SLICE_POINTER + POINTER_READ + BOX_FROM_RAW + 11;
// assume_init and from_boxed_utf8_unchecked each perform a raw-pointer round trip.
const MESSAGE_BOX_ROUND_TRIPS_WORK: usize = 2 * CALL_1 + 2 * BOX_INTO_RAW + 2 * BOX_FROM_RAW + 6;
const SUBDIAGNOSTIC_CONSTRUCTION_WORK: usize = CALL_2 + CALL_1 + EMPTY_VECTOR_CONSTRUCTION
    + BOX_ALLOCATION_WORK + 5 + BOX_RETIREMENT_WORK + EMPTY_VECTOR_RETIREMENT;
const SUBDIAGNOSTIC_PUSH_WORK: usize = 4 * CALL_2 + VECTOR_CAPACITY + CALL_1 + 1
    + POINTER_ACCESS + POINTER_ADD_WORK + POINTER_WRITE + 14;

const fn nonnull_new_unchecked_quote<T: ?Sized>() -> FixedQuote {
    event_quote(NONNULL_NEW_UNCHECKED_WORK, &[
        size_of::<*mut T>(), size_of::<NonNull<T>>(), size_of::<(*const u8,)>(),
        size_of::<usize>(), size_of::<bool>(),
    ])
}

const NONNULL_NEW_UNCHECKED_QUOTE: FixedQuote = nonnull_new_unchecked_quote::<u8>();

const MESSAGE_CONVERSION: FixedQuote = add_quotes(add_quotes(
    event_quote(MESSAGE_ENTRY_WORK, &[
        size_of::<String>(), size_of::<Vec<u8>>(), size_of::<ManuallyDrop<Vec<u8>>>(),
        size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
    ]),
    add_quotes(
        // RawVec::into_box receives its owned buffer and length together. The containing
        // Vec bounds the private buffer; subsequent checks do not carry that argument pair.
        event_quote(CALL_2, &[size_of::<(Vec<u8>, usize)>()]),
        event_quote(MESSAGE_RAW_BUFFER_WORK - CALL_2, &[
            size_of::<Vec<u8>>(), size_of::<ManuallyDrop<Vec<u8>>>(),
            size_of::<Box<[MaybeUninit<u8>]>>(), size_of::<(*mut u8, usize)>(),
            size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
        ]),
    ),
), add_quotes(
    event_quote(MESSAGE_BOX_ROUND_TRIPS_WORK, &[
        size_of::<Box<[MaybeUninit<u8>]>>(), size_of::<Box<[u8]>>(), size_of::<Box<str>>(),
        size_of::<ManuallyDrop<Box<[u8]>>>(), size_of::<*mut [u8]>(),
        size_of::<(*const (), usize, bool)>(), size_of::<usize>(), size_of::<bool>(),
    ]),
    add_quotes(
        event_quote(CALL_1 + 2, &[size_of::<Box<str>>(), size_of::<DiagnosticMessage>()]),
        // The boxed backing's retirement retains its complete pointer/Layout/allocator groups.
        event_quote(BOX_RETIREMENT_WORK, &[
            size_of::<Box<str>>(), size_of::<Layout>(), size_of::<*mut str>(),
            size_of::<(NonNull<u8>, Layout, *const ())>(), size_of::<(*const (), usize, bool)>(),
            size_of::<usize>(), size_of::<bool>(),
        ]),
    ),
));

const UNIQUE_MUTATION: FixedQuote = {
    const COMPARE_EXCHANGE: usize = 2 * CALL_5 + 11;
    const COMPARE_EXCHANGE_INTRINSIC: usize = CALL_3 + 1;
    const LOAD: usize = 2 * CALL_2 + CALL_1 + 3;
    const STORE: usize = 2 * CALL_3 + CALL_2 + 3;
    // Only the atomic calls carry their complete ordering/value argument groups. The
    // surrounding unique-Arc checks move thin pointers, scalar values and scalar results.
    add_quotes(add_quotes(
        event_quote(COMPARE_EXCHANGE, &[
            size_of::<(&AtomicUsize, usize, usize, Ordering, Ordering)>(),
            size_of::<(*mut usize, usize, usize, Ordering, Ordering)>(),
            size_of::<Result<usize, usize>>(),
        ]),
        event_quote(COMPARE_EXCHANGE_INTRINSIC, &[
            size_of::<(*mut usize, usize, usize)>(), size_of::<(usize, bool)>(),
        ]),
    ), add_quotes(add_quotes(
        event_quote(LOAD, &[
            size_of::<(&AtomicUsize, Ordering)>(), size_of::<(*mut usize, Ordering)>(),
            size_of::<usize>(),
        ]),
        event_quote(STORE, &[
            size_of::<(&AtomicUsize, usize, Ordering)>(),
            size_of::<(*mut usize, usize, Ordering)>(), size_of::<usize>(),
        ]),
    ), event_quote(
        UNIQUE_MUTATION_WORK - COMPARE_EXCHANGE - COMPARE_EXCHANGE_INTRINSIC - LOAD - STORE,
        &[
            size_of::<Result<usize, usize>>(), size_of::<(usize, bool)>(),
            size_of::<*mut ()>(), size_of::<usize>(), size_of::<bool>(),
        ],
    )))
};

const SUBDIAGNOSTIC_CONSTRUCTION: FixedQuote = add_quotes(
    event_quote(SUBDIAGNOSTIC_CONSTRUCTION_WORK, &[
        size_of::<SubDiagnostic>(), size_of::<(SubDiagnosticSeverity, DiagnosticMessage)>(),
        size_of::<Vec<Annotation>>(), size_of::<(NonNull<u8>, Layout, *const ())>(),
        size_of::<(Layout, bool, *const ())>(), size_of::<(NonNull<[u8]>, bool)>(),
        size_of::<(*const (), usize, bool)>(), size_of::<(*mut Annotation, usize)>(),
        size_of::<(usize, usize)>(), size_of::<u32>(),
    ]),
    // Private aggregate construction, Box::new argument, write_via_move argument, and heap.
    fixed_quote(0, [(4, SubDiagnostic::allocation_layout().size())]),
);

const SUBDIAGNOSTIC_PUSH: FixedQuote = event_quote(SUBDIAGNOSTIC_PUSH_WORK, &[
    size_of::<(&mut Vec<SubDiagnostic>, SubDiagnostic)>(),
    size_of::<(*mut SubDiagnostic, SubDiagnostic)>(), size_of::<(*const (), usize, usize)>(),
    size_of::<(*const (), usize, bool)>(), size_of::<(usize, bool)>(),
    size_of::<Option<usize>>(), size_of::<u32>(),
]);

/// Quotes moving a String with length equal to capacity into a DiagnosticMessage.
///
/// No buffer is allocated or copied. Conversion carriers and boxed-buffer retirement are
/// included; fixed helper captures/results and quote preparation are separate.
pub(in crate::types::infer::builder) const fn exact_filled_string_message_quote() -> RunResult<(usize, usize)> {
    match MESSAGE_CONVERSION {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint message conversion quotation overflow")),
    }
}

/// Quotes Arc::make_mut for a diagnostic with one strong reference and no explicit weak refs.
///
/// Callers separately quote the selected field mutation and quote preparation.
pub(in crate::types::infer::builder) const fn unique_diagnostic_mutation_quote() -> RunResult<(usize, usize)> {
    match UNIQUE_MUTATION {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("unique diagnostic mutation quotation overflow")),
    }
}

/// Quotes constructing a subdiagnostic from an owned message with prepaid buffer retirement.
///
/// The new boxed record and empty annotation vector's retirement are included. Fixed helper
/// captures/results and quote preparation are separate.
pub(in crate::types::infer::builder) const fn subdiagnostic_construction_quote() -> RunResult<(usize, usize)> {
    match SUBDIAGNOSTIC_CONSTRUCTION {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("subdiagnostic construction quotation overflow")),
    }
}

/// Quotes Diagnostic::info for a unique diagnostic with a spare subdiagnostic slot.
///
/// The message is owned and has prepaid retirement. The unique Arc path, boxed subdiagnostic
/// and reserved insertion are included; quote preparation is separate.
pub(in crate::types::infer::builder) const fn diagnostic_info_quote() -> RunResult<(usize, usize)> {
    const QUOTE: FixedQuote = add_quotes(add_quotes(SUBDIAGNOSTIC_CONSTRUCTION, UNIQUE_MUTATION), SUBDIAGNOSTIC_PUSH);
    match QUOTE {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("diagnostic information quotation overflow")),
    }
}
