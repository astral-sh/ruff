//! Types for extracting and representing comments of a syntax tree.
//!
//! Most programming languages support comments allowing programmers to document their programs. Comments are different from other syntaxes  because programming languages allow comments in almost any position, giving programmers great flexibility on where they can write comments:
//!
//! ```ignore
//! /**
//!  * Documentation comment
//!  */
//! async /* comment */ function Test () // line comment
//! {/*inline*/}
//! ```
//!
//! However, this flexibility makes formatting comments challenging because:
//! * The formatter must consistently place comments so that re-formatting the output yields the same result and does not create invalid syntax (line comments).
//! * It is essential that formatters place comments close to the syntax the programmer intended to document. However, the lack of rules regarding where comments are allowed and what syntax they document requires the use of heuristics to infer the documented syntax.
//!
//! This module strikes a balance between placing comments as closely as possible to their source location and reducing the complexity of formatting comments. It does so by associating comments per node rather than a token. This greatly reduces the combinations of possible comment positions but turns out to be, in practice, sufficiently precise to keep comments close to their source location.
//!
//! ## Node comments
//!
//! Comments are associated per node but get further distinguished on their location related to that node:
//!
//! ### Leading Comments
//!
//! A comment at the start of a node
//!
//! ```ignore
//! // Leading comment of the statement
//! console.log("test");
//!
//! [/* leading comment of identifier */ a ];
//! ```
//!
//! ### Dangling Comments
//!
//! A comment that is neither at the start nor the end of a node
//!
//! ```ignore
//! [/* in between the brackets */ ];
//! async  /* between keywords */  function Test () {}
//! ```
//!
//! ### Trailing Comments
//!
//! A comment at the end of a node
//!
//! ```ignore
//! [a /* trailing comment of a */, b, c];
//! [
//!     a // trailing comment of a
//! ]
//! ```
//!
//! ## Limitations
//! Limiting the placement of comments to leading, dangling, or trailing node comments reduces complexity inside the formatter but means, that the formatter's possibility of where comments can be formatted depends on the AST structure.
//!
//! For example, the continue statement in JavaScript is defined as:
//!
//! ```ungram
//! JsContinueStatement =
//! 'continue'
//! (label: 'ident')?
//! ';'?
//! ```
//!
//! but a programmer may decide to add a comment in front or after the label:
//!
//! ```ignore
//! continue /* comment 1 */ label;
//! continue label /* comment 2*/; /* trailing */
//! ```
//!
//! Because all children of the `continue` statement are tokens, it is only possible to make the comments leading, dangling, or trailing comments of the `continue` statement. But this results in a loss of information as the formatting code can no longer distinguish if a comment appeared before or after the label and, thus, has to format them the same way.
//!
//! This hasn't shown to be a significant limitation today but the infrastructure could be extended to support a `label` on [`SourceComment`] that allows to further categorise comments.
//!

mod builder;
mod map;

use self::{builder::CommentsBuilderVisitor, map::CommentsMap};
use crate::formatter::Formatter;
use crate::{buffer::Buffer, write};
use crate::{CstFormatContext, FormatResult, FormatRule, TextSize, TransformSourceMap};
use ruff_rowan::syntax::SyntaxElementKey;
use ruff_rowan::{Language, SyntaxNode, SyntaxToken, SyntaxTriviaPieceComments};
use rustc_hash::FxHashSet;
#[cfg(debug_assertions)]
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CommentKind {
    /// An inline comment that can appear between any two tokens and doesn't contain any line breaks.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// a /* test */
    /// ```
    InlineBlock,

    /// A block comment that can appear between any two tokens and contains at least one line break.
    ///
    /// ## Examples
    ///
    /// ```javascript
    /// /* first line
    ///  * more content on the second line
    ///  */
    /// ```
    Block,

    /// A line comment that appears at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// a // test
    /// ```
    Line,
}

impl CommentKind {
    pub const fn is_line(&self) -> bool {
        matches!(self, CommentKind::Line)
    }

    pub const fn is_block(&self) -> bool {
        matches!(self, CommentKind::Block)
    }

    pub const fn is_inline_block(&self) -> bool {
        matches!(self, CommentKind::InlineBlock)
    }

    /// Returns `true` for comments that can appear inline between any two tokens.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use ruff_formatter::comments::CommentKind;
    ///
    /// // Block and InlineBlock comments can appear inline
    /// assert!(CommentKind::Block.is_inline());
    /// assert!(CommentKind::InlineBlock.is_inline());
    ///
    /// // But not line comments
    /// assert!(!CommentKind::Line.is_inline())
    /// ```
    pub const fn is_inline(&self) -> bool {
        matches!(self, CommentKind::InlineBlock | CommentKind::Block)
    }
}

/// A comment in the source document.
#[derive(Debug, Clone)]
pub struct SourceComment<L: Language> {
    /// The number of lines appearing before this comment
    pub(crate) lines_before: u32,

    pub(crate) lines_after: u32,

    /// The comment piece
    pub(crate) piece: SyntaxTriviaPieceComments<L>,

    /// The kind of the comment.
    pub(crate) kind: CommentKind,

    /// Whether the comment has been formatted or not.
    #[cfg(debug_assertions)]
    pub(crate) formatted: Cell<bool>,
}

impl<L: Language> SourceComment<L> {
    /// Returns the underlining comment trivia piece
    pub fn piece(&self) -> &SyntaxTriviaPieceComments<L> {
        &self.piece
    }

    /// The number of lines between this comment and the **previous** token or comment.
    ///
    /// # Examples
    ///
    /// ## Same line
    ///
    /// ```ignore
    /// a // end of line
    /// ```
    ///
    /// Returns `0` because there's no line break between the token `a` and the comment.
    ///
    /// ## Own Line
    ///
    /// ```ignore
    /// a;
    ///
    /// /* comment */
    /// ```
    ///
    /// Returns `2` because there are two line breaks between the token `a` and the comment.
    pub fn lines_before(&self) -> u32 {
        self.lines_before
    }

    /// The number of line breaks right after this comment.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```ignore
    /// a; // comment
    ///
    /// b;
    /// ```
    ///
    /// Returns `2` because there are two line breaks between the comment and the token `b`.
    ///
    /// ## Same line
    ///
    /// ```ignore
    /// a;
    /// /* comment */ b;
    /// ```
    ///
    /// Returns `0` because there are no line breaks between the comment and the token `b`.
    pub fn lines_after(&self) -> u32 {
        self.lines_after
    }

    /// The kind of the comment
    pub fn kind(&self) -> CommentKind {
        self.kind
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub fn mark_formatted(&self) {}

    /// Marks the comment as formatted
    #[cfg(debug_assertions)]
    pub fn mark_formatted(&self) {
        self.formatted.set(true)
    }
}

/// A comment decorated with additional information about its surrounding context in the source document.
///
/// Used by [CommentStyle::place_comment] to determine if this should become a [leading](self#leading-comments), [dangling](self#dangling-comments), or [trailing](self#trailing-comments) comment.
#[derive(Debug, Clone)]
pub struct DecoratedComment<L: Language> {
    enclosing: SyntaxNode<L>,
    preceding: Option<SyntaxNode<L>>,
    following: Option<SyntaxNode<L>>,
    following_token: Option<SyntaxToken<L>>,
    text_position: CommentTextPosition,
    lines_before: u32,
    lines_after: u32,
    comment: SyntaxTriviaPieceComments<L>,
    kind: CommentKind,
}

impl<L: Language> DecoratedComment<L> {
    /// The closest parent node that fully encloses the comment.
    ///
    /// A node encloses a comment when the comment is between two of its direct children (ignoring lists).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// [a, /* comment */ b]
    /// ```
    ///
    /// The enclosing node is the array expression and not the identifier `b` because
    /// `a` and `b` are children of the array expression and `comment` is a comment between the two nodes.
    pub fn enclosing_node(&self) -> &SyntaxNode<L> {
        &self.enclosing
    }

    /// Returns the comment piece.
    pub fn piece(&self) -> &SyntaxTriviaPieceComments<L> {
        &self.comment
    }

    /// Returns the node preceding the comment.
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
    /// ```ignore
    /// [/* comment */]
    /// ```
    /// Returns [None] because the comment has no preceding node, only a preceding `[` token.
    ///
    /// ## Preceding node
    ///
    /// ```ignore
    /// [a /* comment */, b]
    /// ```
    ///
    /// Returns `Some(a)` because `a` directly precedes the comment.
    ///
    /// ## Preceding token and node
    ///
    /// ```ignore
    /// [a, /* comment */]
    /// ```
    ///
    ///  Returns `Some(a)` because `a` is the preceding node of `comment`. The presence of the `,` token
    /// doesn't change that.
    pub fn preceding_node(&self) -> Option<&SyntaxNode<L>> {
        self.preceding.as_ref()
    }

    /// Takes the [`preceding_node`](DecoratedComment::preceding_node) and replaces it with [None].
    fn take_preceding_node(&mut self) -> Option<SyntaxNode<L>> {
        self.preceding.take()
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
    /// ```ignore
    /// [ /* comment */ ]
    /// ```
    ///
    /// Returns [None] because there's no node following the comment, only the `]` token.
    ///
    /// ## Following node
    ///
    /// ```ignore
    /// [ /* comment */ a ]
    /// ```
    ///
    /// Returns `Some(a)` because `a` is the node directly following the comment.
    ///
    /// ## Following token and node
    ///
    /// ```ignore
    /// async /* comment */ function test() {}
    /// ```
    ///
    /// Returns `Some(test)` because the `test` identifier is the first node following `comment`.
    ///
    /// ## Following parenthesized expression
    ///
    /// ```ignore
    /// !(
    ///     a /* comment */
    /// );
    /// b
    /// ```
    ///
    /// Returns `None` because `comment` is enclosed inside the parenthesized expression and it has no children
    /// following `/* comment */.
    pub fn following_node(&self) -> Option<&SyntaxNode<L>> {
        self.following.as_ref()
    }

    /// Takes the [`following_node`](DecoratedComment::following_node) and replaces it with [None].
    fn take_following_node(&mut self) -> Option<SyntaxNode<L>> {
        self.following.take()
    }

    /// The number of line breaks between this comment and the **previous** token or comment.
    ///
    /// # Examples
    ///
    /// ## Same line
    ///
    /// ```ignore
    /// a // end of line
    /// ```
    ///
    /// Returns `0` because there's no line break between the token `a` and the comment.
    ///
    /// ## Own Line
    ///
    /// ```ignore
    /// a;
    ///
    /// /* comment */
    /// ```
    ///
    /// Returns `2` because there are two line breaks between the token `a` and the comment.
    pub fn lines_before(&self) -> u32 {
        self.lines_before
    }

    /// The number of line breaks right after this comment.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```ignore
    /// a; // comment
    ///
    /// b;
    /// ```
    ///
    /// Returns `2` because there are two line breaks between the comment and the token `b`.
    ///
    /// ## Same line
    ///
    /// ```ignore
    /// a;
    /// /* comment */ b;
    /// ```
    ///
    /// Returns `0` because there are no line breaks between the comment and the token `b`.
    pub fn lines_after(&self) -> u32 {
        self.lines_after
    }

    /// Returns the [CommentKind] of the comment.
    pub fn kind(&self) -> CommentKind {
        self.kind
    }

    /// The position of the comment in the text.
    pub fn text_position(&self) -> CommentTextPosition {
        self.text_position
    }

    /// The next token that comes after this comment. It is possible that other comments are between this comment
    /// and the token.
    ///
    /// ```ignore
    /// a /* comment */ /* other b */
    /// ```
    ///
    /// The `following_token` for both comments is `b` because it's the token coming after the comments.
    pub fn following_token(&self) -> Option<&SyntaxToken<L>> {
        self.following_token.as_ref()
    }
}

impl<L: Language> From<DecoratedComment<L>> for SourceComment<L> {
    fn from(decorated: DecoratedComment<L>) -> Self {
        Self {
            lines_before: decorated.lines_before,
            lines_after: decorated.lines_after,
            piece: decorated.comment,
            kind: decorated.kind,
            #[cfg(debug_assertions)]
            formatted: Cell::new(false),
        }
    }
}

/// The position of a comment in the source text.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CommentTextPosition {
    /// A comment that is on the same line as the preceding token and is separated by at least one line break from the following token.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```ignore
    /// a; /* this */ // or this
    /// b;
    /// ```
    ///
    /// Both `/* this */` and `// or this` are end of line comments because both comments are separated by
    /// at least one line break from the following token `b`.
    ///
    /// ## Own line
    ///
    /// ```ignore
    /// a;
    /// /* comment */
    /// b;
    /// ```
    ///
    /// This is not an end of line comment because it isn't on the same line as the preceding token `a`.
    EndOfLine,

    /// A Comment that is separated by at least one line break from the preceding token.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// a;
    /// /* comment */ /* or this */
    /// b;
    /// ```
    ///
    /// Both comments are own line comments because they are separated by one line break from the preceding
    /// token `a`.
    OwnLine,

    /// A comment that is placed on the same line as the preceding and following token.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// a /* comment */ + b
    /// ```
    SameLine,
}

impl CommentTextPosition {
    pub const fn is_same_line(&self) -> bool {
        matches!(self, CommentTextPosition::SameLine)
    }

    pub const fn is_own_line(&self) -> bool {
        matches!(self, CommentTextPosition::OwnLine)
    }

    pub const fn is_end_of_line(&self) -> bool {
        matches!(self, CommentTextPosition::EndOfLine)
    }
}

#[derive(Debug)]
pub enum CommentPlacement<L: Language> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    Leading {
        node: SyntaxNode<L>,
        comment: SourceComment<L>,
    },
    /// Makes `comment` a [trailing comment](self#trailing-comments) of `node`.
    Trailing {
        node: SyntaxNode<L>,
        comment: SourceComment<L>,
    },

    /// Makes `comment` a [dangling comment](self#dangling-comments) of `node`.
    Dangling {
        node: SyntaxNode<L>,
        comment: SourceComment<L>,
    },

    /// Uses the default heuristic to determine the placement of the comment.
    ///
    /// # Same line comments
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
    /// ```ignore
    /// [
    ///     a, // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```ignore
    /// [
    ///     a // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```ignore
    /// [ // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```ignore
    /// [ // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the enclosing array expression because both the [`preceding_node`] and [`following_node`] are [None].
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
    /// ```ignore
    /// [
    ///     a,
    ///     // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```ignore
    /// [
    ///     a
    ///     // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```ignore
    /// [
    ///     // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```ignore
    /// [
    ///     // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the array expression because both [`preceding_node`] and [`following_node`] are [None].
    ///
    ///
    /// # End of line comments
    /// Makes the comment a...
    ///
    /// * [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * or a [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * or a [dangling comment] of the [`enclosing_node`].
    ///
    ///
    /// ## Examples
    ///
    /// ### Comment with leading and preceding nodes
    ///
    /// ```ignore
    /// [a /* comment */, b]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a` because there's no token between the node `a` and the `comment`.
    ///
    /// ```ignore
    /// [a, /* comment */ b]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b` because the node `a` and the comment are separated by a `,` token.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```ignore
    /// [a, /* last */ ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a` because the [`following_node`] is [None].
    ///
    /// ### Comment with following node only
    ///
    /// ```ignore
    /// [/* comment */ b]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b` because the [`preceding_node`] is [None]
    ///
    /// ### Dangling comment
    ///
    /// ```ignore
    /// [/* comment*/]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the array expression because both [`preceding_node`] and [`following_node`] are [None].
    ///
    /// [`preceding_node`]: DecoratedComment::preceding_node
    /// [`following_node`]: DecoratedComment::following_node
    /// [`enclosing_node`]: DecoratedComment::enclosing_node
    /// [trailing comment]: self#trailing-comments
    /// [leading comment]: self#leading-comments
    /// [dangling comment]: self#dangling-comments
    Default(DecoratedComment<L>),
}

impl<L: Language> CommentPlacement<L> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    #[inline]
    pub fn leading(node: SyntaxNode<L>, comment: impl Into<SourceComment<L>>) -> Self {
        Self::Leading {
            node,
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [dangling comment](self::dangling-comments) of `node`.
    pub fn dangling(node: SyntaxNode<L>, comment: impl Into<SourceComment<L>>) -> Self {
        Self::Dangling {
            node,
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [trailing comment](self::trailing-comments) of `node`.
    #[inline]
    pub fn trailing(node: SyntaxNode<L>, comment: impl Into<SourceComment<L>>) -> Self {
        Self::Trailing {
            node,
            comment: comment.into(),
        }
    }

    /// Returns the placement if it isn't [CommentPlacement::Default], otherwise calls `f` and returns the result.
    #[inline]
    pub fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce(DecoratedComment<L>) -> CommentPlacement<L>,
    {
        match self {
            CommentPlacement::Default(comment) => f(comment),
            placement => placement,
        }
    }
}

/// Defines how to format comments for a specific [Language].
pub trait CommentStyle: Default {
    type Language: Language;

    /// Returns `true` if a comment with the given `text` is a `rome-ignore format:` suppression comment.
    fn is_suppression(_text: &str) -> bool {
        false
    }

    /// Returns the (kind)[CommentKind] of the comment
    fn get_comment_kind(comment: &SyntaxTriviaPieceComments<Self::Language>) -> CommentKind;

    /// Determines the placement of `comment`.
    ///
    /// The default implementation returns [CommentPlacement::Default].
    fn place_comment(
        &self,
        comment: DecoratedComment<Self::Language>,
    ) -> CommentPlacement<Self::Language> {
        CommentPlacement::Default(comment)
    }
}

/// The comments of a syntax tree stored by node.
///
/// Cloning `comments` is cheap as it only involves bumping a reference counter.
#[derive(Debug, Clone, Default)]
pub struct Comments<L: Language> {
    /// The use of a [Rc] is necessary to achieve that [Comments] has a lifetime that is independent from the [crate::Formatter].
    /// Having independent lifetimes is necessary to support the use case where a (formattable object)[crate::Format]
    /// iterates over all comments, and writes them into the [crate::Formatter] (mutably borrowing the [crate::Formatter] and in turn its context).
    ///
    /// ```block
    /// for leading in f.context().comments().leading_comments(node) {
    ///     ^
    ///     |- Borrows comments
    ///   write!(f, [comment(leading.piece.text())])?;
    ///          ^
    ///          |- Mutably borrows the formatter, state, context, and comments (if comments aren't cloned)
    /// }
    /// ```
    ///
    /// Using an `Rc` here allows to cheaply clone [Comments] for these use cases.
    data: Rc<CommentsData<L>>,
}

impl<L: Language> Comments<L> {
    /// Extracts all the comments from `root` and its descendants nodes.
    pub fn from_node<Style>(
        root: &SyntaxNode<L>,
        style: &Style,
        source_map: Option<&TransformSourceMap>,
    ) -> Self
    where
        Style: CommentStyle<Language = L>,
    {
        let builder = CommentsBuilderVisitor::new(style, source_map);

        let (comments, skipped) = builder.visit(root);

        Self {
            data: Rc::new(CommentsData {
                root: Some(root.clone()),
                is_suppression: Style::is_suppression,

                comments,
                with_skipped: skipped,
                #[cfg(debug_assertions)]
                checked_suppressions: RefCell::new(Default::default()),
            }),
        }
    }

    /// Returns `true` if the given `node` has any [leading](self#leading-comments) or [trailing](self#trailing-comments) comments.
    #[inline]
    pub fn has_comments(&self, node: &SyntaxNode<L>) -> bool {
        self.data.comments.has(&node.key())
    }

    /// Returns `true` if the given `node` has any [leading comments](self#leading-comments).
    #[inline]
    pub fn has_leading_comments(&self, node: &SyntaxNode<L>) -> bool {
        !self.leading_comments(node).is_empty()
    }

    /// Tests if the node has any [leading comments](self#leading-comments) that have a leading line break.
    ///
    /// Corresponds to [CommentTextPosition::OwnLine].
    pub fn has_leading_own_line_comment(&self, node: &SyntaxNode<L>) -> bool {
        self.leading_comments(node)
            .iter()
            .any(|comment| comment.lines_after() > 0)
    }

    /// Returns the `node`'s [leading comments](self#leading-comments).
    #[inline]
    pub fn leading_comments(&self, node: &SyntaxNode<L>) -> &[SourceComment<L>] {
        self.data.comments.leading(&node.key())
    }

    /// Returns `true` if node has any [dangling comments](self#dangling-comments).
    pub fn has_dangling_comments(&self, node: &SyntaxNode<L>) -> bool {
        !self.dangling_comments(node).is_empty()
    }

    /// Returns the [dangling comments](self#dangling-comments) of `node`
    pub fn dangling_comments(&self, node: &SyntaxNode<L>) -> &[SourceComment<L>] {
        self.data.comments.dangling(&node.key())
    }

    /// Returns the `node`'s [trailing comments](self#trailing-comments).
    #[inline]
    pub fn trailing_comments(&self, node: &SyntaxNode<L>) -> &[SourceComment<L>] {
        self.data.comments.trailing(&node.key())
    }

    /// Returns `true` if the node has any [trailing](self#trailing-comments) [line](CommentKind::Line) comment.
    pub fn has_trailing_line_comment(&self, node: &SyntaxNode<L>) -> bool {
        self.trailing_comments(node)
            .iter()
            .any(|comment| comment.kind().is_line())
    }

    /// Returns `true` if the given `node` has any [trailing comments](self#trailing-comments).
    #[inline]
    pub fn has_trailing_comments(&self, node: &SyntaxNode<L>) -> bool {
        !self.trailing_comments(node).is_empty()
    }

    /// Returns an iterator over the [leading](self#leading-comments) and [trailing comments](self#trailing-comments) of `node`.
    pub fn leading_trailing_comments(
        &self,
        node: &SyntaxNode<L>,
    ) -> impl Iterator<Item = &SourceComment<L>> {
        self.leading_comments(node)
            .iter()
            .chain(self.trailing_comments(node).iter())
    }

    /// Returns an iterator over the [leading](self#leading-comments), [dangling](self#dangling-comments), and [trailing](self#trailing) comments of `node`.
    pub fn leading_dangling_trailing_comments<'a>(
        &'a self,
        node: &'a SyntaxNode<L>,
    ) -> impl Iterator<Item = &SourceComment<L>> + 'a {
        self.data.comments.parts(&node.key())
    }

    /// Returns `true` if that node has skipped token trivia attached.
    #[inline]
    pub fn has_skipped(&self, token: &SyntaxToken<L>) -> bool {
        self.data.with_skipped.contains(&token.key())
    }

    /// Returns `true` if `node` has a [leading](self#leading-comments), [dangling](self#dangling-comments), or [trailing](self#trailing-comments) suppression comment.
    ///
    /// # Examples
    ///
    /// ```javascript
    /// // rome-ignore format: Reason
    /// console.log("Test");
    /// ```
    ///
    /// Returns `true` for the expression statement but `false` for the call expression because the
    /// call expression is nested inside of the expression statement.
    pub fn is_suppressed(&self, node: &SyntaxNode<L>) -> bool {
        self.mark_suppression_checked(node);
        let is_suppression = self.data.is_suppression;

        self.leading_dangling_trailing_comments(node)
            .any(|comment| is_suppression(comment.piece().text()))
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub fn mark_suppression_checked(&self, _: &SyntaxNode<L>) {}

    /// Marks that it isn't necessary for the given node to check if it has been suppressed or not.
    #[cfg(debug_assertions)]
    pub fn mark_suppression_checked(&self, node: &SyntaxNode<L>) {
        let mut checked_nodes = self.data.checked_suppressions.borrow_mut();
        checked_nodes.insert(node.clone());
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub(crate) fn assert_checked_all_suppressions(&self, _: &SyntaxNode<L>) {}

    /// Verifies that [NodeSuppressions::is_suppressed] has been called for every node of `root`.
    /// This is a no-op in builds that have the feature `debug_assertions` disabled.
    ///
    /// # Panics
    /// If theres any node for which the formatting didn't very if it has a suppression comment.
    #[cfg(debug_assertions)]
    pub(crate) fn assert_checked_all_suppressions(&self, root: &SyntaxNode<L>) {
        use ruff_rowan::SyntaxKind;

        let checked_nodes = self.data.checked_suppressions.borrow();
        for node in root.descendants() {
            if node.kind().is_list() || node.kind().is_root() {
                continue;
            }

            if !checked_nodes.contains(&node) {
                panic!(
                    r#"
The following node has been formatted without checking if it has suppression comments.
Ensure that the formatter calls into the node's formatting rule by using `node.format()` or
manually test if the node has a suppression comment using `f.context().comments().is_suppressed(node.syntax())`
if using the node's format rule isn't an option."

Node:
{node:#?}"#
                );
            }
        }
    }

    #[inline(always)]
    #[cfg(not(debug_assertions))]
    pub(crate) fn assert_formatted_all_comments(&self) {}

    #[cfg(debug_assertions)]
    pub(crate) fn assert_formatted_all_comments(&self) {
        let has_unformatted_comments = self
            .data
            .comments
            .all_parts()
            .any(|comment| !comment.formatted.get());

        if has_unformatted_comments {
            let mut unformatted_comments = Vec::new();

            for node in self
                .data
                .root
                .as_ref()
                .expect("Expected root for comments with data")
                .descendants()
            {
                unformatted_comments.extend(self.leading_comments(&node).iter().filter_map(
                    |comment| {
                        (!comment.formatted.get()).then_some(DebugComment::Leading {
                            node: node.clone(),
                            comment,
                        })
                    },
                ));
                unformatted_comments.extend(self.dangling_comments(&node).iter().filter_map(
                    |comment| {
                        (!comment.formatted.get()).then_some(DebugComment::Dangling {
                            node: node.clone(),
                            comment,
                        })
                    },
                ));
                unformatted_comments.extend(self.trailing_comments(&node).iter().filter_map(
                    |comment| {
                        (!comment.formatted.get()).then_some(DebugComment::Trailing {
                            node: node.clone(),
                            comment,
                        })
                    },
                ));
            }

            panic!("The following comments have not been formatted.\n{unformatted_comments:#?}")
        }
    }
}

struct CommentsData<L: Language> {
    root: Option<SyntaxNode<L>>,

    is_suppression: fn(&str) -> bool,

    /// Stores all leading node comments by node
    comments: CommentsMap<SyntaxElementKey, SourceComment<L>>,
    with_skipped: FxHashSet<SyntaxElementKey>,

    /// Stores all nodes for which [Comments::is_suppressed] has been called.
    /// This index of nodes that have been checked if they have a suppression comments is used to
    /// detect format implementations that manually format a child node without previously checking if
    /// the child has a suppression comment.
    ///
    /// The implementation refrains from snapshotting the checked nodes because a node gets formatted
    /// as verbatim if its formatting fails which has the same result as formatting it as suppressed node
    /// (thus, guarantees that the formatting isn't changed).
    #[cfg(debug_assertions)]
    checked_suppressions: RefCell<FxHashSet<SyntaxNode<L>>>,
}

impl<L: Language> Default for CommentsData<L> {
    fn default() -> Self {
        Self {
            root: None,
            is_suppression: |_| false,
            comments: Default::default(),
            with_skipped: Default::default(),
            #[cfg(debug_assertions)]
            checked_suppressions: Default::default(),
        }
    }
}

impl<L: Language> std::fmt::Debug for CommentsData<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut comments = Vec::new();

        if let Some(root) = &self.root {
            for node in root.descendants() {
                for leading in self.comments.leading(&node.key()) {
                    comments.push(DebugComment::Leading {
                        node: node.clone(),
                        comment: leading,
                    });
                }

                for dangling in self.comments.dangling(&node.key()) {
                    comments.push(DebugComment::Dangling {
                        node: node.clone(),
                        comment: dangling,
                    });
                }

                for trailing in self.comments.trailing(&node.key()) {
                    comments.push(DebugComment::Trailing {
                        node: node.clone(),
                        comment: trailing,
                    });
                }
            }
        }

        comments.sort_by_key(|comment| comment.start());

        f.debug_list().entries(comments).finish()
    }
}

/// Helper for printing a comment of [Comments]
enum DebugComment<'a, L: Language> {
    Leading {
        comment: &'a SourceComment<L>,
        node: SyntaxNode<L>,
    },
    Trailing {
        comment: &'a SourceComment<L>,
        node: SyntaxNode<L>,
    },
    Dangling {
        comment: &'a SourceComment<L>,
        node: SyntaxNode<L>,
    },
}

impl<L: Language> DebugComment<'_, L> {
    fn start(&self) -> TextSize {
        match self {
            DebugComment::Leading { comment, .. }
            | DebugComment::Trailing { comment, .. }
            | DebugComment::Dangling { comment, .. } => comment.piece.text_range().start(),
        }
    }
}

impl<L: Language> std::fmt::Debug for DebugComment<'_, L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebugComment::Leading { node, comment } => f
                .debug_struct("Leading")
                .field("node", node)
                .field("comment", comment)
                .finish(),
            DebugComment::Dangling { node, comment } => f
                .debug_struct("Dangling")
                .field("node", node)
                .field("comment", comment)
                .finish(),
            DebugComment::Trailing { node, comment } => f
                .debug_struct("Trailing")
                .field("node", node)
                .field("comment", comment)
                .finish(),
        }
    }
}

/// Formats a comment as it was in the source document
pub struct FormatPlainComment<C> {
    context: PhantomData<C>,
}

impl<C> Default for FormatPlainComment<C> {
    fn default() -> Self {
        FormatPlainComment {
            context: PhantomData,
        }
    }
}

impl<C> FormatRule<SourceComment<C::Language>> for FormatPlainComment<C>
where
    C: CstFormatContext,
{
    type Context = C;

    fn fmt(
        &self,
        item: &SourceComment<C::Language>,
        f: &mut Formatter<Self::Context>,
    ) -> FormatResult<()> {
        write!(f, [item.piece.as_piece()])
    }
}
