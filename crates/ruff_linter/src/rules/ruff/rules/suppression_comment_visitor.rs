use std::iter::Peekable;

use ruff_python_ast::{
    visitor::preorder::{self, PreorderVisitor, TraversalSignal},
    AnyNodeRef, Suite,
};
use ruff_python_trivia::{
    indentation_at_offset, CommentLinePosition, SimpleTokenKind, SimpleTokenizer, SuppressionKind,
};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

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
            .map_or(true, |next| next.range.start() >= node_end)
    }
}

impl<'ast, I> PreorderVisitor<'ast> for SuppressionCommentVisitor<'ast, '_, I>
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
                if line_position.is_own_line() {
                    if let Some(token) =
                        SimpleTokenizer::starts_at(node_end, self.locator.contents())
                            .skip_trivia()
                            .next()
                    {
                        if token.end() <= range.end() {
                            break;
                        }
                    }
                    let comment_indent = indentation_at_offset(range.start(), self.locator)
                        .unwrap_or_default()
                        .len();
                    let node_indent = indentation_at_offset(node.start(), self.locator)
                        .unwrap_or_default()
                        .len();
                    if node_indent >= comment_indent {
                        break;
                    }
                } else {
                    if self.locator.line_start(range.start())
                        != self.locator.line_start(node.start())
                    {
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
                    preorder::walk_body(self, body);
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SuppressionCommentData<'src> {
    /// If `enclosing` is `None`, this comment is top-level
    pub(super) enclosing: Option<AnyNodeRef<'src>>,
    pub(super) preceding: Option<AnyNodeRef<'src>>,
    pub(super) following: Option<AnyNodeRef<'src>>,

    pub(super) line_position: CommentLinePosition,
    pub(super) kind: SuppressionKind,
    pub(super) range: TextRange,
}

pub(super) trait CaptureSuppressionComment<'src> {
    /// This is the entrypoint for the capturer to analyze the next comment.
    fn capture(&mut self, comment: SuppressionCommentData<'src>);
}

/// Determine the indentation level of an own-line comment, defined as the minimum indentation of
/// all comments between the preceding node and the comment, including the comment itself. In
/// other words, we don't allow successive comments to ident _further_ than any preceding comments.
///
/// For example, given:
/// ```python
/// if True:
///     pass
///     # comment
/// ```
///
/// The indentation would be 4, as the comment is indented by 4 spaces.
///
/// Given:
/// ```python
/// if True:
///     pass
/// # comment
/// else:
///     pass
/// ```
///
/// The indentation would be 0, as the comment is not indented at all.
///
/// Given:
/// ```python
/// if True:
///     pass
///     # comment
///         # comment
/// ```
///
/// Both comments would be marked as indented at 4 spaces, as the indentation of the first comment
/// is used for the second comment.
///
/// This logic avoids pathological cases like:
/// ```python
/// try:
///     if True:
///         if True:
///             pass
///
///         # a
///             # b
///         # c
/// except Exception:
///     pass
/// ```
///
/// If we don't use the minimum indentation of any preceding comments, we would mark `# b` as
/// indented to the same depth as `pass`, which could in turn lead to us treating it as a trailing
/// comment of `pass`, despite there being a comment between them that "resets" the indentation.
pub(super) fn own_line_comment_indentation(
    preceding: AnyNodeRef,
    comment_range: TextRange,
    locator: &Locator,
) -> TextSize {
    let tokenizer = SimpleTokenizer::new(
        locator.contents(),
        TextRange::new(locator.full_line_end(preceding.end()), comment_range.end()),
    );

    tokenizer
        .filter_map(|token| {
            if token.kind() == SimpleTokenKind::Comment {
                indentation_at_offset(token.start(), locator).map(TextLen::text_len)
            } else {
                None
            }
        })
        .min()
        .unwrap_or_default()
}
