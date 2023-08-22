use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{TextRange, TextSize};

use crate::node::AnyNodeRef;
use crate::{ExpressionRef, Ranged};

/// Returns the [`TextRange`] of a given expression including parentheses, if the expression is
/// parenthesized; or `None`, if the expression is not parenthesized.
pub fn parenthesized_range(
    expr: ExpressionRef,
    parent: AnyNodeRef,
    contents: &str,
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
        parent.end() - TextSize::new(1)
    } else {
        parent.end()
    };

    // First, test if there's a closing parenthesis because it tends to be cheaper.
    let tokenizer =
        SimpleTokenizer::new(contents, TextRange::new(expr.end(), exclusive_parent_end));
    let right = tokenizer.skip_trivia().next()?;

    if right.kind == SimpleTokenKind::RParen {
        // Next, test for the opening parenthesis.
        let mut tokenizer =
            SimpleTokenizer::up_to_without_back_comment(expr.start(), contents).skip_trivia();
        let left = tokenizer.next_back()?;
        if left.kind == SimpleTokenKind::LParen {
            return Some(TextRange::new(left.start(), right.end()));
        }
    }

    None
}
