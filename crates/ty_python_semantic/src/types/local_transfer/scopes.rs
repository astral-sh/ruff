//! Fixed quotations for borrowing indexed scopes and advancing their ancestor cursor.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::AncestorsIter;
use ty_python_core::scope::{FileScopeId, Scope};

use super::collections::{CALL_1, CALL_2, POINTER_ACCESS, event_quote};

// IndexVec::deref/as_slice and Vec::deref/as_slice borrow the backing vector. The
// transparent IndexSlice conversion preserves its slice pointer and length.
const BORROW_SCOPES: usize = 5 * CALL_1 + POINTER_ACCESS + CALL_2 + 12;
// The generated FileScopeId index follows index/as_usize/as_u32/NonZero::get.
// Slice indexing then performs its bounds check and the in-bounds intrinsic access.
const INDEX_SCOPE: usize = 3 * CALL_2 + 5 * CALL_1 + 16;

const fn quote(work: usize) -> RunResult<(usize, usize)> {
    match event_quote(
        work,
        &[
            size_of::<AncestorsIter<'static>>(),
            size_of::<(&[Scope], FileScopeId)>(),
            size_of::<Option<(FileScopeId, &Scope)>>(),
            size_of::<(*const Scope, usize, usize, bool)>(),
            size_of::<Option<FileScopeId>>(),
        ],
    ) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("scope cursor quotation overflow")),
    }
}

/// Quotes construction of the borrowed ancestor iterator, including its initial scope ID.
pub(in crate::types) const fn ancestors_quote() -> RunResult<(usize, usize)> {
    quote(BORROW_SCOPES + 2 * CALL_2 + 10)
}

/// Quotes one ancestor step, including indexed access and the stored parent ID.
pub(in crate::types) const fn next_ancestor_quote() -> RunResult<(usize, usize)> {
    quote(INDEX_SCOPE + 2 * CALL_1 + 22)
}

/// Quotes reading an indexed scope's parent without traversing the ancestor chain.
pub(in crate::types) const fn scope_parent_quote() -> RunResult<(usize, usize)> {
    quote(BORROW_SCOPES + INDEX_SCOPE + CALL_2 + CALL_1 + 8)
}
