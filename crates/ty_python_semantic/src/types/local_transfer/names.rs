//! Admission for borrowed name text and bytewise name comparisons.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

#[cfg(target_pointer_width = "32")]
use super::collections::POINTER_READ;
use super::collections::{
    CALL_1, CALL_2, CALL_3, CALL_4, CHECK_LANGUAGE_UB, MAYBE_IS_ALIGNED_AND_NOT_NULL,
    POINTER_ADD_WORK, RAW_SLICE_POINTER, event_quote,
};

// char_str 0.0.4 reads the tag and the second machine word on 64-bit targets. The
// pointer addition includes its debug precondition, and the integer conversion permits
// either target endianness. The 32-bit representation may read a heap-stored length.
#[cfg(target_pointer_width = "64")]
const REPR_LENGTH: usize = 4 * CALL_1 + 2 * CALL_2 + POINTER_ADD_WORK + 28;
#[cfg(target_pointer_width = "32")]
const REPR_LENGTH: usize = 16 * CALL_1 + 4 * CALL_2 + 2 * POINTER_ADD_WORK + POINTER_READ + 60;

// from_raw_parts checks alignment/non-nullness and the isize-sized allocation bound.
// is_valid_allocation_size selects the zero-sized branch or divides isize::MAX by size.
const SLICE_VIEW: usize = CALL_2
    + CHECK_LANGUAGE_UB
    + CALL_4
    + MAYBE_IS_ALIGNED_AND_NOT_NULL
    + CALL_2
    + RAW_SLICE_POINTER
    + 24;
const REPR_BYTES: usize = CALL_1 + REPR_LENGTH + CALL_1 + SLICE_VIEW + 12;
const NAME_BORROW: usize = 5 * CALL_1 + REPR_BYTES + 5;

// Name/CharStr/Repr equality builds two byte views, compares lengths and pointers, then
// uses slice equality. The bytewise specialization calls unchecked_mul and compare_bytes.
const SLICE_EQUALITY: usize = 2 * CALL_2 + 4 * CALL_1 + 2 * CALL_3 + 12;
const NAME_EQUALITY: usize = 4 * CALL_2 + 2 * REPR_BYTES + 4 * CALL_1 + SLICE_EQUALITY + 14;

const fn quote(work: usize) -> RunResult<(usize, usize)> {
    match event_quote(
        work,
        &[
            size_of::<&Name>(),
            size_of::<Name>(),
            size_of::<&str>(),
            size_of::<(*const (), usize, usize, usize)>(),
            size_of::<(*const u8, *const u8, usize)>(),
            size_of::<(usize, usize)>(),
            size_of::<bool>(),
            size_of::<i32>(),
        ],
    ) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract("borrowed name quotation overflow")),
    }
}

/// Quotes `Name::as_str`, including its CharStr length and borrowed slice construction.
/// The returned view borrows existing storage; this operation neither clones the name nor scans its text.
pub(in crate::types) const fn borrowed_name_quote() -> RunResult<(usize, usize)> {
    quote(NAME_BORROW)
}

/// Quotes two borrowed name views, their byte lengths, and the equality quotation arithmetic.
/// Admit the returned quotation before reading the lengths and invoking [`name_comparison_quote`].
pub(in crate::types) const fn name_comparison_preparation_quote() -> RunResult<(usize, usize)> {
    quote(2 * NAME_BORROW + 4 * CALL_1 + 4 * CALL_2 + CALL_1 + 32)
}

/// Quotes name equality for a pre-admitted upper bound on compared UTF-8 bytes.
/// `length` is the shorter name's byte length; unequal lengths and shared pointers can return earlier.
pub(in crate::types) fn name_comparison_quote(length: usize) -> RunResult<(usize, usize)> {
    let (fixed_work, fixed_bytes) = const {
        let views = quote(2 * REPR_BYTES);
        let comparison = event_quote(
            NAME_EQUALITY - 2 * REPR_BYTES,
            &[
                size_of::<(&Name, &Name)>(),
                size_of::<(&[u8], &[u8], usize)>(),
                size_of::<(*const u8, *const u8, usize)>(),
                size_of::<i32>(),
                size_of::<bool>(),
            ],
        );
        match (views, comparison) {
            (Ok((view_work, view_bytes)), Some((work, bytes))) => {
                match (work.checked_add(view_work), bytes.checked_add(view_bytes)) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract(
                        "name comparison fixed quotation overflow",
                    )),
                }
            }
            (Err(error), _) => Err(error),
            (_, None) => Err(RunError::Contract(
                "name comparison fixed quotation overflow",
            )),
        }
    }?;
    let work = fixed_work
        .checked_add(length)
        .ok_or(RunError::Contract("name comparison work overflow"))?;
    let bytes = length
        .checked_mul(size_of::<(u8, u8, bool)>())
        .and_then(|bytes| bytes.checked_add(fixed_bytes))
        .ok_or(RunError::Contract(
            "name comparison byte quotation overflow",
        ))?;
    Ok((work, bytes))
}
