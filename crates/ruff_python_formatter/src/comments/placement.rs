use crate::comments::visitor::{CommentPlacement, DecoratedComment};
use crate::comments::{CommentTextPosition, SourceComment};
use ruff_formatter::{SourceCode, SourceCodeSlice};
use ruff_python_ast::node::AnyNodeRef;
use std::cell::Cell;

/// Implements the custom comment placement logic.
pub(super) fn place_comment<'a>(
    comment: DecoratedComment<'a>,
    _source_code: SourceCode,
) -> CommentPlacement<'a> {
    CommentPlacement::Default(comment)
}
