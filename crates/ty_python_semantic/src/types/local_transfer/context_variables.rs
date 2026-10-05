//! Fixed quotations for borrowing variables by position from a canonical generic context.

use salsa::execution_probe::{RunError, RunResult};

use super::collections::{CALL_1, CALL_2, POINTER_ACCESS, VECTOR_LENGTH, event_quote};
use crate::types::BoundTypeVarInstance;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::typevar::BoundTypeVarIdentity;

// OrderMap::len, IndexMap::len and Core::len include the debug-build length assertion.
// Core reads Vec::len once and HashTable/RawTable::len twice; 24 direct events cover
// the assertion's references, tuple/match bindings, equality and successful branch.
const CONTEXT_LENGTH: usize = 3 * CALL_1 + VECTOR_LENGTH + 2 * (2 * CALL_1 + 1) + 24;

// IndexMap's borrowed entry slice follows Vec::deref/as_slice/as_ptr and RawVec::ptr.
// The slice access uses the checked SliceIndex implementation and its in-bounds intrinsic.
const BORROW_ENTRIES: usize = 5 * CALL_1 + POINTER_ACCESS + CALL_2 + 6;
const SLICE_GET: usize = 3 * CALL_2 + CALL_1 + 8;
const PAIR_MAP: usize = CALL_2 + CALL_1 + 6;
const VALUE_MAP: usize = CALL_2 + CALL_1 + 5;
const ORDERED_LOOKUP: usize = 3 * CALL_2 + BORROW_ENTRIES + SLICE_GET + PAIR_MAP + VALUE_MAP;

/// Quotes the context map's stored length, including its debug assertion.
pub(in crate::types) const fn context_variable_count_quote() -> RunResult<(usize, usize)> {
    match event_quote(
        CONTEXT_LENGTH,
        &[
            size_of::<&ContextVariables<'static>>(),
            size_of::<(usize, usize)>(),
            size_of::<(&usize, &usize)>(),
            size_of::<bool>(),
        ],
    ) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract(
            "context variable count quotation overflow",
        )),
    }
}

/// Quotes `GenericContext::variable_at_in`, preserving the map's stored order without copying it.
/// Private entry slices use their actual fat-pointer width; their entries remain borrowed.
pub(in crate::types) const fn context_variable_at_quote() -> RunResult<(usize, usize)> {
    match event_quote(
        ORDERED_LOOKUP,
        &[
            size_of::<(&ContextVariables<'static>, usize)>(),
            size_of::<
                Option<(
                    &BoundTypeVarIdentity<'static>,
                    &BoundTypeVarInstance<'static>,
                )>,
            >(),
            size_of::<Option<BoundTypeVarInstance<'static>>>(),
            size_of::<&[()]>(),
            size_of::<(*const (), usize, usize, bool)>(),
            size_of::<Vec<()>>(),
        ],
    ) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract(
            "context variable lookup quotation overflow",
        )),
    }
}
