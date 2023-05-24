#![allow(unused, unreachable_pub)] // TODO(micha): Remove after using the new comments infrastructure in the formatter.

//! Types for extracting and representing comments of a syntax tree.
//!
//! Most programming languages support comments allowing programmers to document their programs.
//! Comments are different from other syntax because programming languages allow comments in almost any position,
//! giving programmers great flexibility on where they can write comments:
//!
//! ```javascript
//! /**
//!  * Documentation comment
//!  */
//! async /* comment */ function Test () // line comment
//! {/*inline*/}
//! ```
//!
//! This flexibility makes formatting comments challenging because:
//! * The formatter must consistently place comments so that re-formatting the output yields the same result,
//!   and does not create invalid syntax (line comments).
//! * It is essential that formatters place comments close to the syntax the programmer intended to document.
//!   However, the lack of rules regarding where comments are allowed and what syntax they document requires
//!   the use of heuristics to infer the documented syntax.
//!
//! This module tries to strike a balance between placing comments as closely as possible to their source location
//! and reducing the complexity of formatting comments. It does so by associating comments per node rather than a token.
//! This greatly reduces the combinations of possible comment positions, but turns out to be, in practice,
//! sufficiently precise to keep comments close to their source location.
//!
//! Luckily, Python doesn't support inline comments, which simplifying the problem significantly.
//!
//! ## Node comments
//!
//! Comments are associated per node but get further distinguished on their location related to that node:
//!
//! ### Leading Comments
//!
//! A comment at the start of a node
//!
//! ```python
//! # Leading comment of the statement
//! print("test");
//!
//! [   # Leading comment of a
//!     a
//! ];
//! ```
//!
//! ### Dangling Comments
//!
//! A comment that is neither at the start nor the end of a node.
//!
//! ```python
//! [
//!     # I'm between two brackets. There are no nodes
//! ];
//! ```
//!
//! ### Trailing Comments
//!
//! A comment at the end of a node.
//!
//! ```python
//! [
//!     a, # trailing comment of a
//!     b, c
//! ];
//! ```
//!
//! ## Limitations
//! Limiting the placement of comments to leading, dangling, or trailing node comments reduces complexity inside the formatter but means,
//! that the formatter's possibility of where comments can be formatted depends on the AST structure.
//!
//! For example, *`RustPython`* doesn't create a node for the `/` operator separating positional only arguments from the other arguments.
//!
//! ```python
//! def test(
//!     a,
//!     /, # The following arguments are positional or named arguments
//!     b
//! ):
//!     pass
//! ```
//!
//! Because *`RustPython`* doesn't create a Node for the `/` argument, it is impossible to associate any
//! comments with it. Meaning, the default behaviour is to associate the `# The following ...` comment
//! with the `b` argument, which is incorrect. This limitation can be worked around by implementing
//! a custom rule to associate comments for `/` as *dangling comments* of the `Arguments` node and then
//! implement custom formatting inside of the arguments formatter.
//!
//! It is possible to add an additional optional label to [`SourceComment`] If ever the need arises to distinguish two *dangling comments* in the formatting logic,

use rustpython_parser::ast::Mod;
use std::cell::Cell;
use std::fmt::{Debug, Formatter};
use std::rc::Rc;

mod debug;
mod map;
mod node_key;
mod placement;
mod visitor;

use crate::comments::debug::{DebugComment, DebugComments};
use crate::comments::map::MultiMap;
use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::visitor::CommentsVisitor;
use ruff_formatter::{SourceCode, SourceCodeSlice};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::source_code::CommentRanges;

/// A comment in the source document.
#[derive(Debug, Clone)]
pub(crate) struct SourceComment {
    /// The location of the comment in the source document.
    pub(super) slice: SourceCodeSlice,

    /// Whether the comment has been formatted or not.
    #[cfg(debug_assertions)]
    pub(super) formatted: Cell<bool>,

    pub(super) position: CommentTextPosition,
}

impl SourceComment {
    /// Returns the location of the comment in the original source code.
    /// Allows retrieving the text of the comment.
    pub(crate) fn slice(&self) -> &SourceCodeSlice {
        &self.slice
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub fn mark_formatted(&self) {}

    /// Marks the comment as formatted
    #[cfg(debug_assertions)]
    pub(crate) fn mark_formatted(&self) {
        self.formatted.set(true);
    }
}

impl SourceComment {
    /// Returns a nice debug representation that prints the source code for every comment (and not just the range).
    pub(crate) fn debug<'a>(&'a self, source_code: SourceCode<'a>) -> DebugComment<'a> {
        DebugComment::new(self, source_code)
    }
}

/// The position of a comment in the source text.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum CommentTextPosition {
    /// A comment that is on the same line as the preceding token and is separated by at least one line break from the following token.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```python
    /// a; # comment
    /// b;
    /// ```
    ///
    /// `# comment` is an end of line comments because it is separated by at least one line break from the following token `b`.
    ///
    /// ## Own line
    ///
    /// ```python
    /// a;
    /// # comment
    /// b;
    /// ```
    ///
    /// `# comment` isn't an end of line comment because it isn't on the same line as the preceding token `a`.
    EndOfLine,

    /// A Comment that is separated by at least one line break from the preceding token.
    ///
    /// # Examples
    ///
    /// ```python
    /// a;
    /// # comment
    /// b;
    /// ```
    ///
    /// `# comment` line comments because they are separated by one line break from the preceding
    /// token `a`.
    OwnLine,
}

impl CommentTextPosition {
    pub(crate) const fn is_own_line(self) -> bool {
        matches!(self, CommentTextPosition::OwnLine)
    }

    pub(crate) const fn is_end_of_line(self) -> bool {
        matches!(self, CommentTextPosition::EndOfLine)
    }
}

type CommentsMap<'a> = MultiMap<NodeRefEqualityKey<'a>, SourceComment>;

/// The comments of a syntax tree stored by node.
///
/// Cloning `comments` is cheap as it only involves bumping a reference counter.
#[derive(Clone, Default)]
pub(crate) struct Comments<'a> {
    /// The implementation uses an [Rc] so that [Comments] has a lifetime independent from the [crate::Formatter].
    /// Independent lifetimes are necessary to support the use case where a (formattable object)[crate::Format]
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
    /// The use of an `Rc` solves this problem because we can cheaply clone `comments` before iterating.
    ///
    /// ```block
    /// let comments = f.context().comments().clone();
    /// for leading in comments.leading_comments(node) {
    ///     write!(f, [comment(leading.piece.text())])?;
    /// }
    /// ```
    data: Rc<CommentsData<'a>>,
}

impl<'a> Comments<'a> {
    fn new(comments: CommentsMap<'a>) -> Self {
        Self {
            data: Rc::new(CommentsData { comments }),
        }
    }

    /// Extracts the comments from the AST.
    pub(crate) fn from_ast(
        root: &'a Mod,
        source_code: SourceCode<'a>,
        comment_ranges: &'a CommentRanges,
    ) -> Self {
        let map = if comment_ranges.is_empty() {
            CommentsMap::new()
        } else {
            CommentsVisitor::new(source_code, comment_ranges).visit(root)
        };

        Self::new(map)
    }

    #[inline]
    pub(crate) fn has_comments(&self, node: AnyNodeRef) -> bool {
        self.data.comments.has(&NodeRefEqualityKey::from_ref(node))
    }

    /// Returns `true` if the given `node` has any [leading comments](self#leading-comments).
    #[inline]
    pub(crate) fn has_leading_comments(&self, node: AnyNodeRef) -> bool {
        !self.leading_comments(node).is_empty()
    }

    /// Returns the `node`'s [leading comments](self#leading-comments).
    #[inline]
    pub(crate) fn leading_comments(&self, node: AnyNodeRef<'a>) -> &[SourceComment] {
        self.data
            .comments
            .leading(&NodeRefEqualityKey::from_ref(node))
    }

    /// Returns `true` if node has any [dangling comments](self#dangling-comments).
    pub(crate) fn has_dangling_comments(&self, node: AnyNodeRef<'a>) -> bool {
        !self.dangling_comments(node).is_empty()
    }

    /// Returns the [dangling comments](self#dangling-comments) of `node`
    pub(crate) fn dangling_comments(&self, node: AnyNodeRef<'a>) -> &[SourceComment] {
        self.data
            .comments
            .dangling(&NodeRefEqualityKey::from_ref(node))
    }

    /// Returns the `node`'s [trailing comments](self#trailing-comments).
    #[inline]
    pub(crate) fn trailing_comments(&self, node: AnyNodeRef<'a>) -> &[SourceComment] {
        self.data
            .comments
            .trailing(&NodeRefEqualityKey::from_ref(node))
    }

    /// Returns `true` if the given `node` has any [trailing comments](self#trailing-comments).
    #[inline]
    pub(crate) fn has_trailing_comments(&self, node: AnyNodeRef) -> bool {
        !self.trailing_comments(node).is_empty()
    }

    /// Returns an iterator over the [leading](self#leading-comments) and [trailing comments](self#trailing-comments) of `node`.
    pub(crate) fn leading_trailing_comments(
        &self,
        node: AnyNodeRef<'a>,
    ) -> impl Iterator<Item = &SourceComment> {
        self.leading_comments(node)
            .iter()
            .chain(self.trailing_comments(node).iter())
    }

    /// Returns an iterator over the [leading](self#leading-comments), [dangling](self#dangling-comments), and [trailing](self#trailing) comments of `node`.
    pub(crate) fn leading_dangling_trailing_comments(
        &self,
        node: AnyNodeRef<'a>,
    ) -> impl Iterator<Item = &SourceComment> {
        self.data
            .comments
            .parts(&NodeRefEqualityKey::from_ref(node))
    }

    #[inline(always)]
    #[cfg(not(debug_assertions))]
    pub(crate) fn assert_formatted_all_comments(&self, _source_code: SourceCode) {}

    #[cfg(debug_assertions)]
    pub(crate) fn assert_formatted_all_comments(&self, source_code: SourceCode) {
        use std::fmt::Write;

        let mut output = String::new();
        let unformatted_comments = self
            .data
            .comments
            .all_parts()
            .filter(|c| !c.formatted.get());

        for comment in unformatted_comments {
            // SAFETY: Writing to a string never fails.
            writeln!(output, "{:#?}", comment.debug(source_code)).unwrap();
        }

        assert!(
            output.is_empty(),
            "The following comments have not been formatted.\n{output}"
        );
    }

    /// Returns an object that implements [Debug] for nicely printing the [`Comments`].
    pub(crate) fn debug(&'a self, source_code: SourceCode<'a>) -> DebugComments<'a> {
        DebugComments::new(&self.data.comments, source_code)
    }
}

#[derive(Default)]
struct CommentsData<'a> {
    comments: CommentsMap<'a>,
}

#[cfg(test)]
mod tests {
    use crate::comments::Comments;
    use insta::assert_debug_snapshot;
    use ruff_formatter::SourceCode;
    use ruff_python_ast::prelude::*;
    use ruff_python_ast::source_code::{CommentRanges, CommentRangesBuilder};
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};

    struct CommentsTestCase<'a> {
        module: Mod,
        comment_ranges: CommentRanges,
        source_code: SourceCode<'a>,
    }

    impl<'a> CommentsTestCase<'a> {
        fn from_code(code: &'a str) -> Self {
            let source_code = SourceCode::new(code);
            let tokens: Vec<_> = lex(code, Mode::Module).collect();

            let mut comment_ranges = CommentRangesBuilder::default();

            for (token, range) in tokens.iter().flatten() {
                comment_ranges.visit_token(token, *range);
            }

            let comment_ranges = comment_ranges.finish();

            let parsed = parse_tokens(tokens.into_iter(), Mode::Module, "test.py")
                .expect("Expect source to be valid Python");

            CommentsTestCase {
                source_code,
                module: parsed,
                comment_ranges,
            }
        }

        fn to_comments(&self) -> Comments {
            Comments::from_ast(&self.module, self.source_code, &self.comment_ranges)
        }
    }

    #[test]
    fn base_test() {
        let source = r#"
# Function Leading comment
def test(x, y):
    if x == y: # if statement end of line comment
        print("Equal")

    # Leading comment
    elif x < y:
        print("Less")
    else:
        print("Greater")

# own line comment

test(10, 20)
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn only_comments() {
        let source = r#"
# Some comment

# another comment
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn empty_file() {
        let source = r#""#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn dangling_comment() {
        let source = r#"
def test(
        # Some comment
    ):
    pass
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn parenthesized_expression() {
        let source = r#"
a = ( # Trailing comment
    10 + # More comments
     3
    )
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn parenthesized_trailing_comment() {
        let source = r#"(
    a
    # comment
)
"#;

        let test_case = CommentsTestCase::from_code(source);
        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn trailing_function_comment() {
        let source = r#"
def test(x, y):
    if x == y:
        pass
    elif x < y:
        print("Less")
    else:
        print("Greater")

    # Trailing comment

test(10, 20)
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn leading_most_outer() {
        let source = r#"
# leading comment
x
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    // Comment should be attached to the statement
    #[test]
    fn trailing_most_outer() {
        let source = r#"
x # trailing comment
y # trailing last node
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn trailing_most_outer_nested() {
        let source = r#"
x + (
    3 # trailing comment
) # outer
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }

    #[test]
    fn trailing_after_comma() {
        let source = r#"
def test(
    a, # Trailing comment for argument `a`
    b,
): pass
"#;
        let test_case = CommentsTestCase::from_code(source);

        let comments = test_case.to_comments();

        assert_debug_snapshot!(comments.debug(test_case.source_code));
    }
}
