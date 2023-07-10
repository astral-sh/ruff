use std::iter::Peekable;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{
    Alias, Arg, ArgWithDefault, Arguments, Comprehension, Decorator, ExceptHandler, Expr, Keyword,
    MatchCase, Mod, Pattern, Ranged, Stmt, WithItem,
};

use ruff_formatter::{SourceCode, SourceCodeSlice};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::source_code::{CommentRanges, Locator};
// The interface is designed to only export the members relevant for iterating nodes in
// pre-order.
#[allow(clippy::wildcard_imports)]
use ruff_python_ast::visitor::preorder::*;
use ruff_python_whitespace::is_python_whitespace;

use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::placement::place_comment;
use crate::comments::{CommentLinePosition, CommentsMap, SourceComment};

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
                parent: self.parents.iter().rev().nth(1).copied(),
                line_position: text_position(*comment_range, self.source_code),
                slice: self.source_code.slice(*comment_range),
            };

            self.builder.add_comment(place_comment(
                comment,
                &Locator::new(self.source_code.as_str()),
            ));
            self.comment_ranges.next();
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

    // Try to skip the subtree if
    // * there are no comments
    // * if the next comment comes after this node (meaning, this nodes subtree contains no comments)
    fn can_skip(&mut self, node_end: TextSize) -> bool {
        self.comment_ranges
            .peek()
            .map_or(true, |next_comment| next_comment.start() >= node_end)
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
                parent: self.parents.last().copied(),
                following: None,
                line_position: text_position(*comment_range, self.source_code),
                slice: self.source_code.slice(*comment_range),
            };

            self.builder.add_comment(place_comment(
                comment,
                &Locator::new(self.source_code.as_str()),
            ));

            self.comment_ranges.next();
        }

        self.preceding_node = Some(node);
    }

    fn finish(self) -> CommentsMap<'a> {
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

    fn visit_body(&mut self, body: &'ast [Stmt]) {
        match body {
            [] => {
                // no-op
            }
            [only] => self.visit_stmt(only),
            [first, .., last] => {
                if self.can_skip(last.end()) {
                    // Skip traversing the body when there's no comment between the first and last statement.
                    // It is still necessary to visit the first statement to process all comments between
                    // the previous node and the first statement.
                    self.visit_stmt(first);
                    self.preceding_node = Some(last.into());
                } else {
                    walk_body(self, body);
                }
            }
        }
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

    fn visit_decorator(&mut self, decorator: &'ast Decorator) {
        if self.start_node(decorator).is_traverse() {
            walk_decorator(self, decorator);
        }
        self.finish_node(decorator);
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

    fn visit_except_handler(&mut self, except_handler: &'ast ExceptHandler) {
        if self.start_node(except_handler).is_traverse() {
            walk_except_handler(self, except_handler);
        }
        self.finish_node(except_handler);
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

    fn visit_arg_with_default(&mut self, arg_with_default: &'ast ArgWithDefault) {
        if self.start_node(arg_with_default).is_traverse() {
            walk_arg_with_default(self, arg_with_default);
        }
        self.finish_node(arg_with_default);
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

    fn visit_with_item(&mut self, with_item: &'ast WithItem) {
        if self.start_node(with_item).is_traverse() {
            walk_with_item(self, with_item);
        }

        self.finish_node(with_item);
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

fn text_position(comment_range: TextRange, source_code: SourceCode) -> CommentLinePosition {
    let before = &source_code.as_str()[TextRange::up_to(comment_range.start())];

    for c in before.chars().rev() {
        match c {
            '\n' | '\r' => {
                break;
            }
            c if is_python_whitespace(c) => continue,
            _ => return CommentLinePosition::EndOfLine,
        }
    }

    CommentLinePosition::OwnLine
}

/// A comment decorated with additional information about its surrounding context in the source document.
///
/// Used by [`CommentStyle::place_comment`] to determine if this should become a [leading](self#leading-comments), [dangling](self#dangling-comments), or [trailing](self#trailing-comments) comment.
#[derive(Debug, Clone)]
pub(super) struct DecoratedComment<'a> {
    enclosing: AnyNodeRef<'a>,
    preceding: Option<AnyNodeRef<'a>>,
    following: Option<AnyNodeRef<'a>>,
    parent: Option<AnyNodeRef<'a>>,
    line_position: CommentLinePosition,
    slice: SourceCodeSlice,
}

impl<'a> DecoratedComment<'a> {
    /// The closest parent node that fully encloses the comment.
    ///
    /// A node encloses a comment when the comment is between two of its direct children (ignoring lists).
    ///
    /// # Examples
    ///
    /// ```python
    /// [
    ///     a,
    ///     # comment
    ///      b
    /// ]
    /// ```
    ///
    /// The enclosing node is the list expression and not the name `b` because
    /// `a` and `b` are children of the list expression and `comment` is between the two nodes.
    pub(super) fn enclosing_node(&self) -> AnyNodeRef<'a> {
        self.enclosing
    }

    /// Returns the parent of the enclosing node, if any
    pub(super) fn enclosing_parent(&self) -> Option<AnyNodeRef<'a>> {
        self.parent
    }

    /// Returns the slice into the source code.
    pub(super) fn slice(&self) -> &SourceCodeSlice {
        &self.slice
    }

    /// Returns the comment's preceding node.
    ///
    /// The direct child node (ignoring lists) of the [`enclosing_node`](DecoratedComment::enclosing_node) that precedes this comment.
    ///
    /// Returns [None] if the [`enclosing_node`](DecoratedComment::enclosing_node) only consists of tokens or if
    /// all preceding children of the [`enclosing_node`](DecoratedComment::enclosing_node) have been tokens.
    ///
    /// The Preceding node is guaranteed to be a sibling of [`following_node`](DecoratedComment::following_node).
    ///
    /// # Examples
    ///
    /// ## Preceding tokens only
    ///
    /// ```python
    /// [
    ///     # comment
    /// ]
    /// ```
    /// Returns [None] because the comment has no preceding node, only a preceding `[` token.
    ///
    /// ## Preceding node
    ///
    /// ```python
    /// a # comment
    /// b
    /// ```
    ///
    /// Returns `Some(a)` because `a` directly precedes the comment.
    ///
    /// ## Preceding token and node
    ///
    /// ```python
    /// [
    ///     a, # comment
    ///     b
    /// ]
    /// ```
    ///
    ///  Returns `Some(a)` because `a` is the preceding node of `comment`. The presence of the `,` token
    /// doesn't change that.
    pub(super) fn preceding_node(&self) -> Option<AnyNodeRef<'a>> {
        self.preceding
    }

    /// Returns the node following the comment.
    ///
    /// The direct child node (ignoring lists) of the [`enclosing_node`](DecoratedComment::enclosing_node) that follows this comment.
    ///
    /// Returns [None] if the [`enclosing_node`](DecoratedComment::enclosing_node) only consists of tokens or if
    /// all children children of the [`enclosing_node`](DecoratedComment::enclosing_node) following this comment are tokens.
    ///
    /// The following node is guaranteed to be a sibling of [`preceding_node`](DecoratedComment::preceding_node).
    ///
    /// # Examples
    ///
    /// ## Following tokens only
    ///
    /// ```python
    /// [
    ///     # comment
    /// ]
    /// ```
    ///
    /// Returns [None] because there's no node following the comment, only the `]` token.
    ///
    /// ## Following node
    ///
    /// ```python
    /// [ # comment
    ///     a
    /// ]
    /// ```
    ///
    /// Returns `Some(a)` because `a` is the node directly following the comment.
    ///
    /// ## Following token and node
    ///
    /// ```python
    /// [
    ///     a # comment
    ///     , b
    /// ]
    /// ```
    ///
    /// Returns `Some(b)` because the `b` identifier is the first node following `comment`.
    ///
    /// ## Following parenthesized expression
    ///
    /// ```python
    /// (
    ///     a
    ///     # comment
    /// )
    /// b
    /// ```
    ///
    /// Returns `None` because `comment` is enclosed inside the parenthesized expression and it has no children
    /// following `# comment`.
    pub(super) fn following_node(&self) -> Option<AnyNodeRef<'a>> {
        self.following
    }

    /// The position of the comment in the text.
    pub(super) fn line_position(&self) -> CommentLinePosition {
        self.line_position
    }
}

impl From<DecoratedComment<'_>> for SourceComment {
    fn from(decorated: DecoratedComment) -> Self {
        Self::new(decorated.slice, decorated.line_position)
    }
}

#[derive(Debug)]
pub(super) enum CommentPlacement<'a> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    Leading {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },
    /// Makes `comment` a [trailing comment](self#trailing-comments) of `node`.
    Trailing {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },

    /// Makes `comment` a [dangling comment](self#dangling-comments) of `node`.
    Dangling {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },

    /// Uses the default heuristic to determine the placement of the comment.
    ///
    /// # End of line comments
    ///
    /// Makes the comment a...
    ///
    /// * [trailing comment] of the [`preceding_node`] if both the [`following_node`] and [`preceding_node`] are not [None]
    ///     and the comment and [`preceding_node`] are only separated by a space (there's no token between the comment and [`preceding_node`]).
    /// * [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * [dangling comment] of the [`enclosing_node`].
    ///
    /// ## Examples
    /// ### Comment with preceding and following nodes
    ///
    /// ```python
    /// [
    ///     a, # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```python
    /// [
    ///     a # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```python
    /// [ # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```python
    /// [ # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the enclosing list expression because both the [`preceding_node`] and [`following_node`] are [None].
    ///
    /// # Own line comments
    ///
    /// Makes the comment a...
    ///
    /// * [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * or a [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * or a [dangling comment] of the [`enclosing_node`].
    ///
    /// ## Examples
    ///
    /// ### Comment with leading and preceding nodes
    ///
    /// ```python
    /// [
    ///     a,
    ///     # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```python
    /// [
    ///     a
    ///     # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```python
    /// [
    ///     # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```python
    /// [
    ///     # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the list expression because both [`preceding_node`] and [`following_node`] are [None].
    ///
    /// [`preceding_node`]: DecoratedComment::preceding_node
    /// [`following_node`]: DecoratedComment::following_node
    /// [`enclosing_node`]: DecoratedComment::enclosing_node_id
    /// [trailing comment]: self#trailing-comments
    /// [leading comment]: self#leading-comments
    /// [dangling comment]: self#dangling-comments
    Default(DecoratedComment<'a>),
}

impl<'a> CommentPlacement<'a> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    #[inline]
    pub(super) fn leading(node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) -> Self {
        Self::Leading {
            node,
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [dangling comment](self::dangling-comments) of `node`.
    pub(super) fn dangling(node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) -> Self {
        Self::Dangling {
            node,
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [trailing comment](self::trailing-comments) of `node`.
    #[inline]
    pub(super) fn trailing(node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) -> Self {
        Self::Trailing {
            node,
            comment: comment.into(),
        }
    }
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
                match comment.line_position() {
                    CommentLinePosition::EndOfLine => {
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
                    CommentLinePosition::OwnLine => {
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
