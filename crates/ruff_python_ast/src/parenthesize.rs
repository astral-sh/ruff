use ruff_python_trivia::{BackwardsTokenizer, CommentRanges, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::AnyNodeRef;
use crate::ExpressionRef;

/// Returns an iterator over the ranges of the optional parentheses surrounding an expression.
///
/// E.g. for `((f()))` with `f()` as expression, the iterator returns the ranges (1, 6) and (0, 7).
///
/// Note that without a parent the range can be inaccurate, e.g. `f(a)` we falsely return a set of
/// parentheses around `a` even if the parentheses actually belong to `f`. That is why you should
/// generally prefer [`parenthesized_range`].
pub fn parentheses_iterator<'a>(
    expr: ExpressionRef<'a>,
    parent: Option<AnyNodeRef>,
    comment_ranges: &'a CommentRanges,
    source: &'a str,
) -> impl Iterator<Item = TextRange> + 'a {
    let right_tokenizer = if let Some(parent) = parent {
        // If the parent is a node that brings its own parentheses, exclude the closing parenthesis
        // from our search range. Otherwise, we risk matching on calls, like `func(x)`, for which
        // the open and close parentheses are part of the `Arguments` node.
        //
        // There are a few other nodes that may have their own parentheses, but are fine to exclude:
        // - `Parameters`: The parameters to a function definition. Any expressions would represent
        //   default arguments, and so must be preceded by _at least_ the parameter name. As such,
        //   we won't mistake any parentheses for the opening and closing parentheses on the
        //   `Parameters` node itself.
        // - `Tuple`: The elements of a tuple. The only risk is a single-element tuple (e.g., `(x,)`),
        //    which must have a trailing comma anyway.
        let exclusive_parent_end = if parent.is_arguments() {
            parent.end() - ")".text_len()
        } else {
            parent.end()
        };
        SimpleTokenizer::new(source, TextRange::new(expr.end(), exclusive_parent_end))
    } else {
        SimpleTokenizer::starts_at(expr.end(), source)
    };

    let right_tokenizer = right_tokenizer
        .skip_trivia()
        .take_while(|token| token.kind == SimpleTokenKind::RParen);

    let left_tokenizer = BackwardsTokenizer::up_to(expr.start(), source, comment_ranges)
        .skip_trivia()
        .take_while(|token| token.kind == SimpleTokenKind::LParen);

    // Zip closing parenthesis with opening parenthesis. The order is intentional, as testing for
    // closing parentheses is cheaper, and `zip` will avoid progressing the `left_tokenizer` if
    // the `right_tokenizer` is exhausted.
    right_tokenizer
        .zip(left_tokenizer)
        .map(|(right, left)| TextRange::new(left.start(), right.end()))
}

/// Returns the [`TextRange`] of a given expression including parentheses, if the expression is
/// parenthesized; or `None`, if the expression is not parenthesized.
pub fn parenthesized_range(
    expr: ExpressionRef,
    parent: AnyNodeRef,
    comment_ranges: &CommentRanges,
    source: &str,
) -> Option<TextRange> {
    parentheses_iterator(expr, Some(parent), comment_ranges, source).last()
}
