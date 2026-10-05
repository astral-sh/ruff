//! Quotes calculating a table-growth bound before the table operation is admitted.

use salsa::execution_probe::{RunError, RunResult};

use super::StorageQuote;

const CALL_1: usize = 3 + 2;
const CALL_2: usize = 6 + 2;
const CALL_4: usize = 12 + 2;
const CHECKED_ADD: usize = 3 * CALL_2 + CALL_1 + 6;
const CHECKED_MUL: usize = 3 * CALL_2 + CALL_1 + 26;
const SCALAR_MAX: usize = 2 * CALL_2 + 8;
const OPTION_PROPAGATION: usize = CALL_1 + 11;

/// Quotes the arithmetic and temporary carriers used by `table_merge`, including its `slots` calls.
///
/// This only funds calculating the growth quote. Callers separately fund reading collection
/// metadata, adapting the returned quote, and executing the collection operation itself.
pub(in crate::types::infer::builder) const fn table_merge_preparation_quote() -> RunResult<(usize, usize)> {
    let work = CALL_4 + 2 * CALL_1
        + 10 * CHECKED_ADD
        + 7 * CHECKED_MUL
        + 4 * SCALAR_MAX
        + 19 * OPTION_PROPAGATION
        + 45;
    let representations = [
        size_of::<(usize, usize, usize, usize)>(),
        size_of::<StorageQuote>(),
        size_of::<(StorageQuote, usize)>(),
        size_of::<Option<(StorageQuote, usize)>>(),
        size_of::<RunResult<(StorageQuote, usize)>>(),
        size_of::<(usize, bool)>(),
        size_of::<Option<usize>>(),
    ];
    let mut width = 0;
    let mut index = 0;
    while index < representations.len() {
        if representations[index] > width {
            width = representations[index];
        }
        index += 1;
    }
    match work.checked_mul(width) {
        Some(bytes) => Ok((work, bytes)),
        None => Err(RunError::Contract("table preparation quotation overflow")),
    }
}
