use crate::comments::Comments;
use crate::trivia::{first_non_trivia_token, first_non_trivia_token_rev, Token, TokenKind};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::Ranged;

pub(crate) trait NeedsParentheses {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses;
}

pub(super) fn default_expression_needs_parentheses(
    node: AnyNodeRef,
    parenthesize: Parenthesize,
    source: &str,
    comments: &Comments,
) -> Parentheses {
    debug_assert!(
        node.is_expression(),
        "Should only be called for expressions"
    );

    // `Optional` or `Preserve` and expression has parentheses in source code.
    if !parenthesize.is_if_breaks() && is_expression_parenthesized(node, source) {
        Parentheses::Always
    }
    // `Optional` or `IfBreaks`: Add parentheses if the expression doesn't fit on a line but enforce
    // parentheses if the expression has leading comments
    else if !parenthesize.is_preserve() {
        if comments.has_leading_comments(node) {
            Parentheses::Always
        } else {
            Parentheses::Optional
        }
    } else {
        //`Preserve` and expression has no parentheses in the source code
        Parentheses::Never
    }
}

/// Configures if the expression should be parenthesized.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum Parenthesize {
    /// Parenthesize the expression if it has parenthesis in the source.
    #[default]
    Preserve,

    /// Parenthesizes the expression if it doesn't fit on a line OR if the expression is parenthesized in the source code.
    Optional,

    /// Parenthesizes the expression only if it doesn't fit on a line.
    IfBreaks,
}

impl Parenthesize {
    pub(crate) const fn is_if_breaks(self) -> bool {
        matches!(self, Parenthesize::IfBreaks)
    }

    pub(crate) const fn is_preserve(self) -> bool {
        matches!(self, Parenthesize::Preserve)
    }
}

/// Whether it is necessary to add parentheses around an expression.
/// This is different from [`Parenthesize`] in that it is the resolved representation: It takes into account
/// whether there are parentheses in the source code or not.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Parentheses {
    /// Always create parentheses
    Always,

    /// Only add parentheses when necessary because the expression breaks over multiple lines.
    Optional,

    /// Custom handling by the node's formatter implementation
    Custom,

    /// Never add parentheses
    Never,
}

fn is_expression_parenthesized(expr: AnyNodeRef, contents: &str) -> bool {
    matches!(
        first_non_trivia_token(expr.end(), contents),
        Some(Token {
            kind: TokenKind::RParen,
            ..
        })
    ) && matches!(
        first_non_trivia_token_rev(expr.start(), contents),
        Some(Token {
            kind: TokenKind::LParen,
            ..
        })
    )
}
