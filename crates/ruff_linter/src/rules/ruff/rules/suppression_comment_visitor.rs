use std::iter::{FilterMap, Peekable};

use ruff_python_ast::{
    visitor::preorder::{self, PreorderVisitor, TraversalSignal},
    AnyNodeRef, Suite,
};
use ruff_python_trivia::{CommentLinePosition, CommentRanges, SuppressionKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

#[derive(Clone, Copy, Debug)]
struct SuppressionComment {
    range: TextRange,
    kind: SuppressionKind,
}

type MapCommentFn<'src> = Box<dyn Fn(&'src TextRange) -> Option<SuppressionComment> + 'src>;
type CommentIter<'src> = Peekable<FilterMap<std::slice::Iter<'src, TextRange>, MapCommentFn<'src>>>;

/// Visitor that captures AST data for suppression comments. This uses a similar approach
/// to `CommentsVisitor` in the formatter crate.
pub(super) struct SuppressionCommentVisitor<'src, 'builder> {
    source: &'src str,
    comments: CommentIter<'src>,

    parents: Vec<AnyNodeRef<'src>>,
    preceding_node: Option<AnyNodeRef<'src>>,

    builder: &'builder mut (dyn CaptureSuppressionComment<'src> + 'src),
}

impl<'src, 'builder> SuppressionCommentVisitor<'src, 'builder> {
    pub(super) fn new(
        source: &'src str,
        comment_ranges: &'src CommentRanges,
        builder: &'builder mut (dyn CaptureSuppressionComment<'src> + 'src),
    ) -> Self {
        let map_fn: MapCommentFn<'_> = Box::new(|range: &'src TextRange| {
            Some(SuppressionComment {
                range: *range,
                kind: SuppressionKind::from_slice(&source[*range])?,
            })
        });
        Self {
            source,
            comments: comment_ranges.iter().filter_map(map_fn).peekable(),
            parents: Vec::default(),
            preceding_node: Option::default(),
            builder,
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

impl<'ast> PreorderVisitor<'ast> for SuppressionCommentVisitor<'ast, '_> {
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

            let data = SuppressionCommentData {
                enclosing: enclosing_node,
                preceding: self.preceding_node,
                following: Some(node),
                parent: self.parents.iter().rev().nth(0).copied(),
                line_position: CommentLinePosition::text_position(range, self.source),
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
            // If the comment starts after this node, break.
            if range.start() >= node_end {
                break;
            }

            let data = SuppressionCommentData {
                enclosing: Some(node),
                preceding: self.preceding_node,
                following: None,
                parent: self.parents.last().copied(),
                line_position: CommentLinePosition::text_position(range, self.source),
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
#[allow(dead_code)]
pub(super) struct SuppressionCommentData<'src> {
    /// If `enclosing` is `None`, this comment is top-level
    pub(super) enclosing: Option<AnyNodeRef<'src>>,
    pub(super) preceding: Option<AnyNodeRef<'src>>,
    pub(super) following: Option<AnyNodeRef<'src>>,
    pub(super) parent: Option<AnyNodeRef<'src>>,
    pub(super) line_position: CommentLinePosition,
    pub(super) kind: SuppressionKind,
    pub(super) range: TextRange,
}

impl<'src> PartialEq for SuppressionCommentData<'src> {
    fn eq(&self, other: &Self) -> bool {
        self.range.start().eq(&other.range.start())
    }
}

impl<'src> Eq for SuppressionCommentData<'src> {}

impl<'src> Ord for SuppressionCommentData<'src> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.range.start().cmp(&other.range.start())
    }
}

impl<'src> PartialOrd for SuppressionCommentData<'src> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'src> Ranged for SuppressionCommentData<'src> {
    fn range(&self) -> TextRange {
        self.range
    }
}

pub(super) trait CaptureSuppressionComment<'src> {
    fn capture(&mut self, comment: SuppressionCommentData<'src>);
}
