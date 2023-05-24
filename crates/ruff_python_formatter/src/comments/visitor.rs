use crate::comments::map::MultiMap;
use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::placement::{place_comment, CommentPlacement, DecoratedComment};
use crate::comments::{CommentTextPosition, CommentsMap, SourceComment};
use ruff_formatter::SourceCode;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::prelude::*;
use ruff_python_ast::source_code::CommentRanges;
// The interface is designed to only export the members relevant for iterating nodes in
// pre-order.
#[allow(clippy::wildcard_imports)]
use ruff_python_ast::visitor::preorder::*;
use ruff_python_ast::whitespace::is_python_whitespace;
use ruff_text_size::TextRange;
use std::cmp::Ordering;
use std::iter::Peekable;

/// Visitor extracting the comments from an AST.
#[derive(Debug, Clone)]
pub(crate) struct CommentsVisitor<'a> {
    builder: CommentsBuilder<'a>,
    source_code: SourceCode<'a>,
    parents: Vec<AnyNodeRef<'a>>,
    preceding_node: Option<AnyNodeRef<'a>>,
    comment_ranges: Peekable<std::slice::Iter<'a, TextRange>>,
}

impl<'a> CommentsVisitor<'a> {
    pub(crate) fn new(source_code: SourceCode<'a>, comment_ranges: &'a CommentRanges) -> Self {
        Self {
            builder: CommentsBuilder::default(),
            source_code,
            parents: Vec::new(),
            preceding_node: None,
            comment_ranges: comment_ranges.iter().peekable(),
        }
    }

    pub(super) fn visit(mut self, root: &'a Mod) -> CommentsMap<'a> {
        self.visit_mod(root);

        self.finish()
    }

    fn start_node<N>(&mut self, node: N) -> TraversalSignal
    where
        N: Into<AnyNodeRef<'a>>,
    {
        self.start_node_impl(node.into())
    }

    fn start_node_impl(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        let node_range = node.range();

        let enclosing_node = self.parents.last().copied().unwrap_or(node);

        // Process all remaining comments that end before this node's start position.
        // If the `preceding` node is set, then it process all comments ending after the `preceding` node
        // and ending before this node's start position
        while let Some(comment_range) = self.comment_ranges.peek().copied() {
            // Exit if the comment is enclosed by this node or comes after it
            if comment_range.end() > node_range.start() {
                break;
            }

            let comment = DecoratedComment {
                enclosing: enclosing_node,
                preceding: self.preceding_node,
                following: Some(node),
                text_position: text_position(*comment_range, self.source_code),
                slice: self.source_code.slice(*comment_range),
            };

            self.builder
                .add_comment(place_comment(comment, self.source_code));
            self.comment_ranges.next();
        }

        // From here on, we're inside of `node`, meaning, we're passed the preceding node.
        self.preceding_node = None;
        self.parents.push(node);

        // Try to skip the subtree if
        // * there are no comments
        // * if the next comment comes after this node (meaning, this nodes subtree contains no comments)
        self.comment_ranges
            .peek()
            .map_or(TraversalSignal::Skip, |next_comment| {
                if node.range().contains(next_comment.start()) {
                    TraversalSignal::Traverse
                } else {
                    TraversalSignal::Skip
                }
            })
    }

    fn finish_node<N>(&mut self, node: N)
    where
        N: Into<AnyNodeRef<'a>>,
    {
        self.finish_node_impl(node.into());
    }

    fn finish_node_impl(&mut self, node: AnyNodeRef<'a>) {
        // We are leaving this node, pop it from the parent stack.
        self.parents.pop();

        let node_end = node.end();
        let is_root = self.parents.is_empty();

        // Process all comments that start after the `preceding` node and end before this node's end.
        while let Some(comment_range) = self.comment_ranges.peek().copied() {
            // If the comment starts after this node, break.
            // If this is the root node and there are comments after the node, attach them to the root node
            // anyway because there's no other node we can attach the comments to (RustPython should include the comments in the node's range)
            if comment_range.start() >= node_end && !is_root {
                break;
            }

            let comment = DecoratedComment {
                enclosing: node,
                preceding: self.preceding_node,
                following: None,
                text_position: text_position(*comment_range, self.source_code),
                slice: self.source_code.slice(*comment_range),
            };

            self.builder
                .add_comment(place_comment(comment, self.source_code));

            self.comment_ranges.next();
        }

        self.preceding_node = Some(node);
    }

    fn finish(mut self) -> CommentsMap<'a> {
        self.builder.finish()
    }
}

impl<'ast> PreorderVisitor<'ast> for CommentsVisitor<'ast> {
    fn visit_mod(&mut self, module: &'ast Mod) {
        if self.start_node(module).is_traverse() {
            walk_module(self, module);
        }
        self.finish_node(module);
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        if self.start_node(stmt).is_traverse() {
            walk_stmt(self, stmt);
        }
        self.finish_node(stmt);
    }

    fn visit_annotation(&mut self, expr: &'ast Expr) {
        if self.start_node(expr).is_traverse() {
            walk_expr(self, expr);
        }
        self.finish_node(expr);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.start_node(expr).is_traverse() {
            walk_expr(self, expr);
        }
        self.finish_node(expr);
    }

    fn visit_comprehension(&mut self, comprehension: &'ast Comprehension) {
        if self.start_node(comprehension).is_traverse() {
            walk_comprehension(self, comprehension);
        }
        self.finish_node(comprehension);
    }

    fn visit_excepthandler(&mut self, excepthandler: &'ast Excepthandler) {
        if self.start_node(excepthandler).is_traverse() {
            walk_excepthandler(self, excepthandler);
        }
        self.finish_node(excepthandler);
    }

    fn visit_format_spec(&mut self, format_spec: &'ast Expr) {
        if self.start_node(format_spec).is_traverse() {
            walk_expr(self, format_spec);
        }
        self.finish_node(format_spec);
    }

    fn visit_arguments(&mut self, arguments: &'ast Arguments) {
        if self.start_node(arguments).is_traverse() {
            walk_arguments(self, arguments);
        }
        self.finish_node(arguments);
    }

    fn visit_arg(&mut self, arg: &'ast Arg) {
        if self.start_node(arg).is_traverse() {
            walk_arg(self, arg);
        }
        self.finish_node(arg);
    }

    fn visit_keyword(&mut self, keyword: &'ast Keyword) {
        if self.start_node(keyword).is_traverse() {
            walk_keyword(self, keyword);
        }
        self.finish_node(keyword);
    }

    fn visit_alias(&mut self, alias: &'ast Alias) {
        if self.start_node(alias).is_traverse() {
            walk_alias(self, alias);
        }
        self.finish_node(alias);
    }

    fn visit_withitem(&mut self, withitem: &'ast Withitem) {
        if self.start_node(withitem).is_traverse() {
            walk_withitem(self, withitem);
        }

        self.finish_node(withitem);
    }
    fn visit_match_case(&mut self, match_case: &'ast MatchCase) {
        if self.start_node(match_case).is_traverse() {
            walk_match_case(self, match_case);
        }
        self.finish_node(match_case);
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        if self.start_node(pattern).is_traverse() {
            walk_pattern(self, pattern);
        }
        self.finish_node(pattern);
    }
}

fn text_position(comment_range: TextRange, source_code: SourceCode) -> CommentTextPosition {
    let before = &source_code.as_str()[TextRange::up_to(comment_range.start())];

    for c in before.chars().rev() {
        match c {
            '\n' | '\r' => {
                break;
            }
            c if is_python_whitespace(c) => continue,
            _ => return CommentTextPosition::EndOfLine,
        }
    }

    CommentTextPosition::OwnLine
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum TraversalSignal {
    Traverse,
    Skip,
}

impl TraversalSignal {
    const fn is_traverse(self) -> bool {
        matches!(self, TraversalSignal::Traverse)
    }
}

#[derive(Clone, Debug, Default)]
struct CommentsBuilder<'a> {
    comments: CommentsMap<'a>,
}

impl<'a> CommentsBuilder<'a> {
    fn add_comment(&mut self, placement: CommentPlacement<'a>) {
        match placement {
            CommentPlacement::Leading { node, comment } => {
                self.push_leading_comment(node, comment);
            }
            CommentPlacement::Trailing { node, comment } => {
                self.push_trailing_comment(node, comment);
            }
            CommentPlacement::Dangling { node, comment } => {
                self.push_dangling_comment(node, comment);
            }
            CommentPlacement::Default(comment) => {
                match comment.text_position() {
                    CommentTextPosition::EndOfLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            (Some(preceding), Some(_)) => {
                                // Attach comments with both preceding and following node to the preceding
                                // because there's a line break separating it from the following node.
                                // ```python
                                // a; # comment
                                // b
                                // ```
                                self.push_trailing_comment(preceding, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                    CommentTextPosition::OwnLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            // Following always wins for a leading comment
                            // ```python
                            // a
                            // // python
                            // b
                            // ```
                            // attach the comment to the `b` expression statement
                            (_, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                }
            }
        }
    }

    fn finish(self) -> CommentsMap<'a> {
        self.comments
    }

    fn push_leading_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments
            .push_leading(NodeRefEqualityKey::from_ref(node), comment.into());
    }

    fn push_dangling_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments
            .push_dangling(NodeRefEqualityKey::from_ref(node), comment.into());
    }

    fn push_trailing_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments
            .push_trailing(NodeRefEqualityKey::from_ref(node), comment.into());
    }
}
