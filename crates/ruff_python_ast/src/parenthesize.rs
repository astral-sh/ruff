use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::node::AnyNodeRef;
use crate::ExpressionRef;

/// Returns the [`TextRange`] of a given expression including parentheses, if the expression is
/// parenthesized; or `None`, if the expression is not parenthesized.
pub fn parenthesized_range(
    expr: ExpressionRef,
    parent: AnyNodeRef,
    comment_ranges: &[TextRange],
    source: &str,
) -> Option<TextRange> {
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

    let right_tokenizer =
        SimpleTokenizer::new(source, TextRange::new(expr.end(), exclusive_parent_end))
            .skip_trivia()
            .take_while(|token| token.kind == SimpleTokenKind::RParen);

    let mut left_cursor = SimpleTokenizer::up_to_without_back_comment(expr.start(), source);
    let left_tokenizer = std::iter::from_fn(move || {
        let token = left_cursor.previous_token(comment_ranges);
        if token.kind() == SimpleTokenKind::LParen {
            Some(token.range())
        } else {
            None
        }
    });

    // Zip closing parenthesis with opening parenthesis. The order is intentional, as testing for
    // closing parentheses is cheaper, and `zip` will avoid progressing the `left_tokenizer` if
    // the `right_tokenizer` is exhausted.
    right_tokenizer
        .zip(left_tokenizer)
        .last()
        .map(|(right, left)| TextRange::new(left.start(), right.end()))
}
