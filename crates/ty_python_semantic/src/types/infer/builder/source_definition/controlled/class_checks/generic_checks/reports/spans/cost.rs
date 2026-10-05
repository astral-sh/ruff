//! Finite source-range selection and canonical binding-result inspection costs.

use ruff_db::diagnostic::Span;
use ruff_python_ast::{AnyRootNodeRef, PythonVersion};
use ruff_text_size::{TextRange, TextSize};
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use crate::types::Type;
use crate::types::local_transfer::collections::{
    CALL_1, CALL_2, CALL_3, NONNULL_AS_PTR, NONNULL_CAST,
    NONNULL_NEW_UNCHECKED_WORK, POINTER_ACCESS, POINTER_ADD_WORK,
};

// These are bounds for successful indexed reads. Failed internal invariants panic in the
// ordinary accessor too; their panic-formatting paths are not semantic report branches.
const SCALAR_INDEX: usize = 2 * CALL_2 + 4;
const BOX_BORROW: usize = CALL_1 + 2;
const SLICE_LENGTH: usize = 2 * CALL_1 + 4;
const SLICE_TAIL: usize = 2 * CALL_2 + 2 * SLICE_LENGTH + CALL_2 + CALL_3 + 2 * CALL_2 + 8;
const ARC_BORROW: usize = 5 * CALL_1 + 10;
const NODE_INDEX: usize = 3 * CALL_1 + 2 * CALL_2 + 10;

// read_bits reads at most two words. The fixed range check, casts, shifts, mask and
// crossing-word branch do not depend on the number of indexed AST nodes.
const READ_BITS: usize = CALL_3 + 5 * CALL_2 + 4 * CALL_1 + 2 * SCALAR_INDEX + 22;
const PACKED_INDEX: usize = CALL_2 + 2 * BOX_BORROW + SCALAR_INDEX + SLICE_TAIL
    + READ_BITS + SCALAR_INDEX + 10 * CALL_1 + 2 * CALL_2 + 40;
const RAW_NODE: usize = 2 * CALL_2 + 8 * CALL_1 + NONNULL_NEW_UNCHECKED_WORK + 20;
const INDEXED_NODE: usize = 2 * CALL_2 + ARC_BORROW + NODE_INDEX + PACKED_INDEX + RAW_NODE + 12;
const AST_REFERENCE: usize = CALL_2 + 8 * CALL_1 + 4 * CALL_2 + 24 + INDEXED_NODE
    + 3 * CALL_1 + 2 * CALL_2 + 8;

// The packed-index tail slice has the widest argument group: a fat pointer plus
// offset and length. The other entries retain concrete conversion/check result widths.
const INDEX_WIDTH: usize = max(size_of::<(&[u64], usize, usize)>(), max(
    size_of::<AnyRootNodeRef<'_>>(), max(size_of::<(ruff_db::files::File, PythonVersion)>(),
    size_of::<Result<&(), ()>>())));

const fn max(left: usize, right: usize) -> usize {
    if left > right { left } else { right }
}

const RANGE_COVER: usize = CALL_2 + 4 * CALL_1 + 2 * (CALL_2 + CALL_2 + 3) + CALL_2 + 5;
const RANGE_WIDTH: usize = size_of::<(TextRange, TextRange)>();

pub(super) const SPAN_WORK: usize = CALL_1 + 2 * CALL_2 + 14;
pub(super) const SPAN_BYTES: usize = SPAN_WORK * size_of::<(Span, Option<TextRange>)>();

// IndexVec's borrowed slice conversion, Idx conversion, indexed read and Scope::node.
pub(super) const SCOPE_NODE_WORK: usize = 10 * CALL_1 + 4 * CALL_2 + POINTER_ACCESS + 24;
pub(super) const SCOPE_NODE_BYTES: usize = SCOPE_NODE_WORK * size_of::<(&[()], usize)>();

const CLASS_HEADER: usize = 12 * CALL_1 + 3 * CALL_2 + 24;
pub(super) const CLASS_RANGE_WORK: usize = AST_REFERENCE + CLASS_HEADER;
pub(super) const CLASS_RANGE_BYTES: usize = AST_REFERENCE * INDEX_WIDTH + CLASS_HEADER * RANGE_WIDTH;

const FUNCTION_HEADER: usize = 4 * CALL_1 + 2 * RANGE_COVER + 12;
pub(super) const FUNCTION_RANGE_WORK: usize = AST_REFERENCE + FUNCTION_HEADER + SPAN_WORK;
pub(super) const FUNCTION_RANGE_BYTES: usize = AST_REFERENCE * INDEX_WIDTH + FUNCTION_HEADER * RANGE_WIDTH + SPAN_BYTES;

// Annotated assignments select up to three indexed nodes and cover two ranges. Other
// DefinitionKind branches fit this bound, including the first nested-declaration entry.
const DEFINITION_SELECT: usize = 18 * CALL_1 + 4 * CALL_2 + 36;
pub(super) const DEFINITION_RANGE_WORK: usize = 3 * AST_REFERENCE + 2 * RANGE_COVER + DEFINITION_SELECT + SPAN_WORK;
pub(super) const DEFINITION_RANGE_BYTES: usize = 3 * AST_REFERENCE * INDEX_WIDTH
    + (2 * RANGE_COVER + DEFINITION_SELECT) * max(RANGE_WIDTH, size_of::<(&[()], usize)>()) + SPAN_BYTES;

// Both dynamic anchors inspect one node; the offset branch additionally derives the
// absolute node index and may shift a string-relative TextRange.
const DYNAMIC_SELECT: usize = 14 * CALL_1 + 6 * CALL_2 + 2 * RANGE_COVER + 36;
pub(super) const DYNAMIC_RANGE_WORK: usize = AST_REFERENCE + DYNAMIC_SELECT + SPAN_WORK;
pub(super) const DYNAMIC_RANGE_BYTES: usize = AST_REFERENCE * INDEX_WIDTH
    + DYNAMIC_SELECT * size_of::<(TextRange, TextSize, Option<TextRange>)>() + SPAN_BYTES;

// A definition query's compact result has zero, one, or a borrowed boxed slice of bindings.
// Its metadata read and checked quote arithmetic execute before the binding traversal.
pub(super) const BINDING_PREPARATION_WORK: usize = 8 * CALL_1 + 4 * CALL_2 + 30;
pub(super) const BINDING_PREPARATION_BYTES: usize = BINDING_PREPARATION_WORK * size_of::<RunResult<(usize, usize)>>();

const ITERATOR_NEW: usize = 6 * CALL_1 + SLICE_LENGTH + NONNULL_CAST + NONNULL_AS_PTR + POINTER_ADD_WORK + 16;
const BINDING_START: usize = ITERATOR_NEW + 18 * CALL_1 + 6 * CALL_2 + 40;
const BINDING_ENTRY: usize = POINTER_ADD_WORK + NONNULL_AS_PTR + NONNULL_NEW_UNCHECKED_WORK
    + 14 * CALL_1 + 10 * CALL_2 + 2 * CALL_3 + 56;
const BINDING_END: usize = 12 * CALL_1 + 4 * CALL_2 + 28;

/// Quotes ordinary binding selection, including every retained entry and the fallback branch.
pub(super) fn binding_quote(entries: usize) -> RunResult<(usize, usize)> {
    let work = entries.checked_mul(BINDING_ENTRY)
        .and_then(|n| n.checked_add(BINDING_START + BINDING_ENTRY + BINDING_END))
        .ok_or(RunError::Contract("class report binding work overflow"))?;
    let width = size_of::<(Option<(Definition<'_>, Type<'_>)>, Definition<'_>, &())>();
    let bytes = work.checked_mul(width)
        .ok_or(RunError::Contract("class report binding bytes overflow"))?;
    Ok((work, bytes))
}
