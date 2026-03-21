use ruff_text_size::{Ranged, TextLen, TextRange};

use super::{TokenKind, Tokens};
use crate::{AnyNodeRef, ExprRef};

/// Returns an iterator over the ranges of the optional parentheses surrounding an expression.
///
/// E.g. for `((f()))` with `f()` as expression, the iterator returns the ranges (1, 6) and (0, 7).
///
/// Note that without a parent the range can be inaccurate, e.g. `f(a)` we falsely return a set of
/// parentheses around `a` even if the parentheses actually belong to `f`. That is why you should
/// generally prefer [`parenthesized_range`].
pub fn parentheses_iterator<'a>(
    expr: ExprRef<'a>,
    parent: Option<AnyNodeRef>,
    tokens: &'a Tokens,
) -> impl Iterator<Item = TextRange> + 'a {
    let after_tokens = if let Some(parent) = parent {
        // If the parent is a node that brings its own parentheses, exclude the closing parenthesis
        // from our search range. Otherwise, we risk matching on calls, like `func(x)`, for which
        // the open and close parentheses are part of the `Arguments` node.
        let exclusive_parent_end = if parent.is_arguments() {
            parent.end() - ")".text_len()
        } else {
            parent.end()
        };

        tokens.in_range(TextRange::new(expr.end(), exclusive_parent_end))
    } else {
        tokens.after(expr.end())
    };

    let right_parens = after_tokens
        .iter()
        .filter(|token| !token.kind().is_trivia())
        .take_while(move |token| token.kind() == TokenKind::Rpar);

    let left_parens = tokens
        .before(expr.start())
        .iter()
        .rev()
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| token.kind() == TokenKind::Lpar);

    right_parens
        .zip(left_parens)
        .map(|(right, left)| TextRange::new(left.start(), right.end()))
}

/// Returns the [`TextRange`] of a given expression including parentheses, if the expression is
/// parenthesized; or `None`, if the expression is not parenthesized.
pub fn parenthesized_range(
    expr: ExprRef,
    parent: AnyNodeRef,
    tokens: &Tokens,
) -> Option<TextRange> {
    parentheses_iterator(expr, Some(parent), tokens).last()
}
