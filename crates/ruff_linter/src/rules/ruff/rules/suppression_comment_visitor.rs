use std::iter::Peekable;

use ruff_python_ast::{
    helpers::comment_indentation_after,
    visitor::source_order::{self, SourceOrderVisitor, TraversalSignal},
    AnyNodeRef, Suite,
};
use ruff_python_trivia::{
    indentation_at_offset, CommentLinePosition, SimpleTokenizer, SuppressionKind,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Locator;

#[derive(Clone, Copy, Debug)]
pub(super) struct SuppressionComment {
    pub(super) range: TextRange,
    pub(super) kind: SuppressionKind,
}

/// Visitor that captures AST data for suppression comments. This uses a similar approach
/// to `CommentsVisitor` in the formatter crate.
pub(super) struct SuppressionCommentVisitor<
    'src,
    'builder,
    I: Iterator<Item = SuppressionComment> + 'src,
> {
    comments: Peekable<I>,

    parents: Vec<AnyNodeRef<'src>>,
    preceding_node: Option<AnyNodeRef<'src>>,

    builder: &'builder mut (dyn CaptureSuppressionComment<'src> + 'src),
    locator: &'src Locator<'src>,
}

impl<'src, 'builder, I> SuppressionCommentVisitor<'src, 'builder, I>
where
    I: Iterator<Item = SuppressionComment> + 'src,
{
    pub(super) fn new(
        comment_ranges: I,
        builder: &'builder mut (dyn CaptureSuppressionComment<'src> + 'src),
        locator: &'src Locator<'src>,
    ) -> Self {
        Self {
            comments: comment_ranges.peekable(),
            parents: Vec::default(),
            preceding_node: Option::default(),
            builder,
            locator,
        }
    }

    pub(super) fn visit(mut self, suite: &'src Suite) {
        self.visit_body(suite.as_slice());
    }

    fn can_skip(&mut self, node_end: TextSize) -> bool {
        self.comments
            .peek()
            .is_none_or(|next| next.range.start() >= node_end)
    }
}

impl<'ast, I> SourceOrderVisitor<'ast> for SuppressionCommentVisitor<'ast, '_, I>
where
    I: Iterator<Item = SuppressionComment> + 'ast,
{
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        let node_range = node.range();

        let enclosing_node = self.parents.last().copied();

        // Process all remaining comments that end before this node's start position.
        // If the `preceding` node is set, then it process all comments ending after the `preceding` node
        // and ending before this node's start position
        while let Some(SuppressionComment { range, kind }) = self.comments.peek().copied() {
            // Exit if the comment is enclosed by this node or comes after it
            if range.end() > node_range.start() {
                break;
            }

            let line_position = CommentLinePosition::for_range(range, self.locator.contents());

            let data = SuppressionCommentData {
                enclosing: enclosing_node,
                preceding: self.preceding_node,
                following: Some(node),
                line_position,
                kind,
                range,
            };

            self.builder.capture(data);
            self.comments.next();
        }

        // From here on, we're inside of `node`, meaning, we're passed the preceding node.
        self.preceding_node = None;
        self.parents.push(node);

        if self.can_skip(node_range.end()) {
            TraversalSignal::Skip
        } else {
            TraversalSignal::Traverse
        }
    }

    fn leave_node(&mut self, node: AnyNodeRef<'ast>) {
        self.parents.pop();

        let node_end = node.end();

        // Process all comments that start after the `preceding` node and end before this node's end.
        while let Some(SuppressionComment { range, kind }) = self.comments.peek().copied() {
            let line_position = CommentLinePosition::for_range(range, self.locator.contents());
            if range.start() >= node_end {
                let between = TextRange::new(node_end, range.start());
                // Check if a non-trivial token exists between the end of this node and the start of the comment.
                // If it doesn't, that means this comment could possibly be a trailing comment that comes after the
                // end of this node.
                // For example:
                // ```
                // def func(x):
                //     pass # fmt: skip
                // ```
                // We want to make sure that `# fmt: skip` is associated with the `pass` statement,
                // even though it comes after the end of that node.
                if SimpleTokenizer::new(self.locator.contents(), between)
                    .skip_trivia()
                    .next()
                    .is_some()
                {
                    break;
                }
                // If the comment is on its own line, it could still be a trailing comment if it has a greater
                // level of indentation compared to this node. For example:
                // ```
                // def func(x):
                //     # fmt: off
                //     pass
                //     # fmt: on
                // def func2(y):
                //     pass
                // ```
                // We want `# fmt: on` to be considered a trailing comment of `func(x)` instead of a leading comment
                // on `func2(y)`.
                if line_position.is_own_line() {
                    let comment_indent =
                        comment_indentation_after(node, range, self.locator.contents());
                    let node_indent = TextSize::of(
                        indentation_at_offset(node.start(), self.locator.contents())
                            .unwrap_or_default(),
                    );
                    if node_indent >= comment_indent {
                        break;
                    }
                }
            }

            let data = SuppressionCommentData {
                enclosing: Some(node),
                preceding: self.preceding_node,
                following: None,
                line_position,
                kind,
                range,
            };

            self.builder.capture(data);
            self.comments.next();
        }

        self.preceding_node = Some(node);
    }
    fn visit_body(&mut self, body: &'ast [ruff_python_ast::Stmt]) {
        match body {
            [] => {
                // no-op
            }
            [only] => {
                self.visit_stmt(only);
            }
            [first, .., last] => {
                if self.can_skip(last.end()) {
                    // Skip traversing the body when there's no comment between the first and last statement.
                    // It is still necessary to visit the first statement to process all comments between
                    // the previous node and the first statement.
                    self.visit_stmt(first);
                    self.preceding_node = Some(last.into());
                } else {
                    source_order::walk_body(self, body);
                }
            }
        }
    }

    fn visit_identifier(&mut self, _identifier: &'ast ruff_python_ast::Identifier) {
        // Skip identifiers, matching the formatter comment extraction
    }
}

#[derive(Clone, Debug)]
pub(super) struct SuppressionCommentData<'src> {
    /// The AST node that encloses the comment. If `enclosing` is `None`, this comment is a top-level statement.
    pub(super) enclosing: Option<AnyNodeRef<'src>>,
    /// An AST node that comes directly before the comment. A child of `enclosing`.
    pub(super) preceding: Option<AnyNodeRef<'src>>,
    /// An AST node that comes directly after the comment. A child of `enclosing`.
    pub(super) following: Option<AnyNodeRef<'src>>,

    /// The line position of the comment - it can either be on its own line, or at the end of a line.
    pub(super) line_position: CommentLinePosition,
    /// Whether this comment is `fmt: off`, `fmt: on`, or `fmt: skip` (or `yapf disable` / `yapf enable`)
    pub(super) kind: SuppressionKind,
    /// The range of text that makes up the comment. This includes the `#` prefix.
    pub(super) range: TextRange,
}

pub(super) trait CaptureSuppressionComment<'src> {
    /// This is the entrypoint for the capturer to analyze the next comment.
    fn capture(&mut self, comment: SuppressionCommentData<'src>);
}
