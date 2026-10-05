//! Quotes the temporary context borrows and metadata used while reporting a lint.

use std::cell::{BorrowError, BorrowMutError, Ref, RefMut};
use std::ptr::NonNull;

use ruff_db::diagnostic::{Diagnostic, LintName, Span, UnifiedFile};
use ruff_db::files::File;
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};

use super::{
    CALL_1, CALL_2, FixedQuote, NONNULL_NEW_UNCHECKED_QUOTE, VECTOR_CAPACITY, add_quotes,
    event_quote,
};
use super::buffers::{BufferQuotePreparation, buffer_quote_preparation};
use crate::types::TypeCheckDiagnostics;
use crate::lint::{LintId, LintMetadata};
use crate::suppression::{FileSuppressionId, Suppression};
use crate::types::context::{InferContext, LintReportMetadata};
use crate::types::context::lint_reporting::EligibleLint;
use crate::types::infer::builder::source_definition::controlled::storage::table_merge_preparation_quote;
use crate::types::storage_quote::StorageQuote;

type SharedDiagnostics = Ref<'static, TypeCheckDiagnostics>;
type UniqueDiagnostics = RefMut<'static, TypeCheckDiagnostics>;

// Each layer adds only its own argument/result carriers and expressions. Calls into a
// lower layer add that layer's quote. The borrow guards are dropped before returning
// from the admitted operation, so restoring the borrow counter belongs to this quote.
const UNSAFE_CELL_GET: FixedQuote = event_quote(
    CALL_1 + 4, &[size_of::<*mut u8>()]);
const CELL_GET: FixedQuote = add_quotes(
    UNSAFE_CELL_GET,
    event_quote(CALL_1 + 3, &[size_of::<*mut u8>(), size_of::<isize>()]),
);
const MEM_REPLACE: FixedQuote = event_quote(
    CALL_2 + CALL_1 + CALL_2 + 6, &[size_of::<(*mut isize, isize)>(), size_of::<isize>()]);
const CELL_REPLACE: FixedQuote = add_quotes(
    add_quotes(UNSAFE_CELL_GET, MEM_REPLACE),
    event_quote(CALL_2 + 4, &[size_of::<(*mut isize, isize)>(), size_of::<isize>()]),
);
const BORROW_FLAG_PREDICATE: FixedQuote = event_quote(
    CALL_1 + 1, &[size_of::<isize>(), size_of::<bool>()]);
const BORROW_NEW: FixedQuote = add_quotes(
    add_quotes(add_quotes(CELL_GET, CELL_REPLACE), BORROW_FLAG_PREDICATE),
    event_quote(
        CALL_1 + 2 * CALL_2 + 11, &[size_of::<*mut u8>(), size_of::<(isize, isize)>(), size_of::<bool>(), size_of::<SharedDiagnostics>(), size_of::<(SharedDiagnostics, bool)>()]),
);
const BORROW_MUT_NEW: FixedQuote = add_quotes(
    add_quotes(CELL_GET, CELL_REPLACE),
    event_quote(
        CALL_1 + 8, &[size_of::<*mut u8>(), size_of::<isize>(), size_of::<bool>(), size_of::<UniqueDiagnostics>(), size_of::<(UniqueDiagnostics, bool)>()]),
);
const BORROW_DROP: FixedQuote = add_quotes(
    add_quotes(add_quotes(CELL_GET, CELL_REPLACE), BORROW_FLAG_PREDICATE),
    event_quote(
        CALL_1 + 8, &[size_of::<*mut u8>(), size_of::<isize>(), size_of::<bool>()]),
);
const TRY_BORROW: FixedQuote = add_quotes(
    add_quotes(add_quotes(BORROW_NEW, UNSAFE_CELL_GET), NONNULL_NEW_UNCHECKED_QUOTE),
    event_quote(
        CALL_1 + 10, &[size_of::<*mut u8>(), size_of::<NonNull<TypeCheckDiagnostics>>(), size_of::<SharedDiagnostics>(), size_of::<Result<SharedDiagnostics, BorrowError>>(), size_of::<bool>()]),
);
const TRY_BORROW_MUT: FixedQuote = add_quotes(
    add_quotes(add_quotes(BORROW_MUT_NEW, UNSAFE_CELL_GET), NONNULL_NEW_UNCHECKED_QUOTE),
    event_quote(
        CALL_1 + 12, &[size_of::<*mut u8>(), size_of::<NonNull<TypeCheckDiagnostics>>(), size_of::<UniqueDiagnostics>(), size_of::<Result<UniqueDiagnostics, BorrowMutError>>(), size_of::<bool>()]),
);
const REFCELL_BORROW: FixedQuote = add_quotes(
    TRY_BORROW,
    event_quote(
        CALL_1 + 3, &[size_of::<*mut u8>(), size_of::<SharedDiagnostics>(), size_of::<bool>()]),
);
const REFCELL_BORROW_MUT: FixedQuote = add_quotes(
    TRY_BORROW_MUT,
    event_quote(
        CALL_1 + 3, &[size_of::<*mut u8>(), size_of::<UniqueDiagnostics>(), size_of::<bool>()]),
);
const REF_DEREFERENCE: FixedQuote = event_quote(
    // Ref::deref -> NonNull::as_ref -> as_ptr/transmute and cast_const. RefMut's
    // deref_mut omits cast_const; summing the shared path bounds either operation.
    CALL_1 + 2 + 4 * CALL_1 + 4, &[size_of::<*mut u8>(), size_of::<NonNull<TypeCheckDiagnostics>>()]);
const REF_DROP: FixedQuote = add_quotes(
    BORROW_DROP,
    event_quote(2, &[size_of::<*mut u8>(), size_of::<SharedDiagnostics>()]),
);
const REF_MUT_DROP: FixedQuote = add_quotes(
    BORROW_DROP,
    event_quote(2, &[size_of::<*mut u8>(), size_of::<UniqueDiagnostics>()]),
);
const VECTOR_METADATA: FixedQuote = event_quote(
    2 * CALL_1 + 7 + VECTOR_CAPACITY, &[size_of::<*mut u8>(), size_of::<usize>(), size_of::<bool>()]);
const SET_METADATA: FixedQuote = event_quote(
    // The std set, hashbrown set/map and raw table each forward len and capacity.
    // The raw table reads items for len and adds items + growth_left for capacity.
    8 * CALL_1 + 5 + 8, &[size_of::<*mut u8>(), size_of::<usize>()]);
const STORAGE: FixedQuote = add_quotes(
    add_quotes(
        add_quotes(REFCELL_BORROW, REF_DEREFERENCE),
        add_quotes(REF_DROP, add_quotes(VECTOR_METADATA, SET_METADATA)),
    ),
    event_quote(
        2 * CALL_1 + 7, &[size_of::<*mut u8>(), size_of::<SharedDiagnostics>(), size_of::<(usize, usize, usize, usize)>()]),
);
const MUTATION: FixedQuote = add_quotes(
    add_quotes(REFCELL_BORROW_MUT, REF_DEREFERENCE),
    add_quotes(
        REF_MUT_DROP,
        event_quote(2 * CALL_2 + 2, &[size_of::<*mut u8>()]),
    ),
);
const METADATA: FixedQuote = event_quote(
    // from(File), with_range and with_optional_range, then the metadata/Option fields.
    CALL_1 + 2 * CALL_2 + 6 + 1 + 4 + 12, &[size_of::<File>(), size_of::<UnifiedFile>(), size_of::<Span>(), size_of::<TextRange>(), size_of::<Option<TextRange>>(), size_of::<LintReportMetadata>(), size_of::<Option<LintReportMetadata>>()]);

const STRING_LENGTH_WORK: usize = 6 * CALL_1 + 3;
const CHECKED_ADD_WORK: usize = 3 * CALL_2 + CALL_1 + 6;
const TEXT_PARTS: FixedQuote = event_quote(
    3 * STRING_LENGTH_WORK + 2 * CHECKED_ADD_WORK + CALL_2 + CALL_1 + 29 + 15 + 3,
    &[
        size_of::<[&str; 3]>(),
        size_of::<((&str, usize), (&str, usize), (&str, usize), usize)>(),
        size_of::<RunResult<((&str, usize), (&str, usize), (&str, usize), usize)>>(),
        size_of::<&[u8]>(),
        size_of::<*const [u8]>(),
        size_of::<(usize, usize)>(),
        size_of::<(usize, bool)>(),
        size_of::<Option<usize>>(),
    ],
);
const TABLE_PREPARATION: FixedQuote = add_quotes(
    match table_merge_preparation_quote() {
        Ok(quote) => Some(quote),
        Err(_) => None,
    },
    event_quote(
        CALL_2 + 7,
        &[
            size_of::<(usize, usize, usize, usize)>(),
            size_of::<StorageQuote>(),
            size_of::<(StorageQuote, usize)>(),
            size_of::<Option<(StorageQuote, usize)>>(),
            size_of::<RunResult<(StorageQuote, usize)>>(),
            size_of::<(usize, bool)>(),
            size_of::<Option<usize>>(),
        ],
    ),
);
const CONTEXT_QUOTE_COMPOSITION: FixedQuote = event_quote(
    5 * CHECKED_ADD_WORK + 2 * 9 + 64,
    &[
        size_of::<usize>(),
        size_of::<bool>(),
        size_of::<StorageQuote>(),
        size_of::<(StorageQuote, usize)>(),
        size_of::<Option<(StorageQuote, usize)>>(),
        size_of::<(Option<(StorageQuote, usize)>, RunError)>(),
        size_of::<RunResult<(StorageQuote, usize)>>(),
        size_of::<(usize, usize)>(),
        size_of::<RunResult<(usize, usize)>>(),
        size_of::<(usize, bool)>(),
        size_of::<Option<usize>>(),
        size_of::<RunError>(),
        size_of::<&str>(),
    ],
);

/// The fixed metadata operation performed inside one reporting admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types::infer::builder) enum ReportingMetadataOperation {
    LintId,
    File,
    SuppressionId,
    Eligible,
    Name,
    NameAndSuffix,
    DocumentationField,
    Verbose,
}

/// Quotes fixed reporting getters and aggregate construction without collection traversal.
///
/// DocumentationField excludes the unique Arc mutation, which has its own shared quote.
pub(in crate::types::infer::builder) const fn reporting_metadata_quote(
    operation: ReportingMetadataOperation,
) -> RunResult<(usize, usize)> {
    const ID: FixedQuote = event_quote(CALL_1 + 3, &[size_of::<&LintMetadata>(), size_of::<LintId>()]);
    const FILE: FixedQuote = event_quote(CALL_1 + 3, &[size_of::<&InferContext<'static, 'static>>(), size_of::<File>()]);
    const SUPPRESSION: FixedQuote = event_quote(CALL_2 + CALL_1 + 7, &[size_of::<Option<&Suppression>>(), size_of::<FileSuppressionId>(), size_of::<Option<FileSuppressionId>>()]);
    const ELIGIBLE: FixedQuote = event_quote(9, &[size_of::<EligibleLint>()]);
    const NAME: FixedQuote = event_quote(3 * (CALL_1 + 3) + 3, &[size_of::<&LintMetadata>(), size_of::<LintName>(), size_of::<&str>()]);
    const NAME_AND_SUFFIX: FixedQuote = event_quote(3 * (CALL_1 + 3) + 3 + CALL_1 + 6, &[size_of::<LintName>(), size_of::<(&str, &str)>()]);
    const DOCUMENTATION: FixedQuote = event_quote(CALL_2 + 7, &[size_of::<(&mut Diagnostic, Option<String>)>(), size_of::<Option<String>>()]);
    const VERBOSE: FixedQuote = event_quote(2, &[size_of::<bool>()]);
    let quote = match operation {
        ReportingMetadataOperation::LintId => ID,
        ReportingMetadataOperation::File => FILE,
        ReportingMetadataOperation::SuppressionId => SUPPRESSION,
        ReportingMetadataOperation::Eligible => ELIGIBLE,
        ReportingMetadataOperation::Name => NAME,
        ReportingMetadataOperation::NameAndSuffix => NAME_AND_SUFFIX,
        ReportingMetadataOperation::DocumentationField => DOCUMENTATION,
        ReportingMetadataOperation::Verbose => VERBOSE,
    };
    match quote {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint reporting metadata quotation overflow")),
    }
}

/// Quotes reading both diagnostic and used-suppression length/capacity through the context.
///
/// The shared borrow and its counter restoration are included. The quoted storage operation
/// returns only collection lengths and capacities.
pub(in crate::types::infer::builder) const fn context_storage_quote() -> RunResult<(usize, usize)> {
    match STORAGE {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint context storage quotation overflow")),
    }
}

/// Quotes borrowing the context mutably, invoking a collection mutation, and releasing the borrow.
///
/// Callers separately quote the collection operation and four representations of its owned
/// argument, for the context and TypeCheckDiagnostics forwarding methods.
pub(in crate::types::infer::builder) const fn context_mutation_quote() -> RunResult<(usize, usize)> {
    match MUTATION {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint context mutation quotation overflow")),
    }
}

/// Quotes constructing lint metadata and its primary span from an eligible lint and a ty file.
pub(in crate::types::infer::builder) const fn lint_metadata_quote() -> RunResult<(usize, usize)> {
    match METADATA {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint metadata quotation overflow")),
    }
}

/// Quotes extracting three borrowed text fragments and summing their UTF-8 lengths.
pub(in crate::types::infer::builder) const fn lint_text_parts_quote() -> RunResult<(usize, usize)> {
    match TEXT_PARTS {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint text parts quotation overflow")),
    }
}

/// Quotes calculating suppression-table growth, retirement and context mutation costs.
pub(in crate::types::infer::builder) const fn suppression_storage_preparation_quote() -> RunResult<(usize, usize)> {
    match add_quotes(TABLE_PREPARATION, CONTEXT_QUOTE_COMPOSITION) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint suppression preparation quotation overflow")),
    }
}

/// Quotes calculating diagnostic-vector reservation and its context mutation costs.
pub(in crate::types::infer::builder) const fn context_reservation_preparation_quote() -> RunResult<(usize, usize)> {
    let collection = match buffer_quote_preparation(BufferQuotePreparation::VecReserveExact) {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    match add_quotes(collection, CONTEXT_QUOTE_COMPOSITION) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("lint reservation preparation quotation overflow")),
    }
}
