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

/// A comment decorated with additional information about its surrounding context in the source document.
///
/// Used by [`CommentStyle::place_comment`] to determine if this should become a [leading](self#leading-comments), [dangling](self#dangling-comments), or [trailing](self#trailing-comments) comment.
#[derive(Debug, Clone)]
pub(super) struct DecoratedComment<'a> {
    pub(super) enclosing: AnyNodeRef<'a>,
    pub(super) preceding: Option<AnyNodeRef<'a>>,
    pub(super) following: Option<AnyNodeRef<'a>>,
    pub(super) text_position: CommentTextPosition,
    pub(super) slice: SourceCodeSlice,
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
    pub(super) fn text_position(&self) -> CommentTextPosition {
        self.text_position
    }
}

impl From<DecoratedComment<'_>> for SourceComment {
    fn from(decorated: DecoratedComment) -> Self {
        Self {
            slice: decorated.slice,
            position: decorated.text_position,
            #[cfg(debug_assertions)]
            formatted: Cell::new(false),
        }
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

    /// Returns the placement if it isn't [`CommentPlacement::Default`], otherwise calls `f` and returns the result.
    #[inline]
    pub(super) fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce(DecoratedComment<'a>) -> CommentPlacement<'a>,
    {
        match self {
            CommentPlacement::Default(comment) => f(comment),
            placement => placement,
        }
    }
}
