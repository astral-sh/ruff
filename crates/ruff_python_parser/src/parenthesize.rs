use ruff_python_ast::{AnyNodeRef, ExprRef};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::{Token, TokenKind};

const fn is_trivia(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Comment
            | TokenKind::NonLogicalNewline
            | TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent
    )
}

/// Returns an iterator over the ranges of the optional parentheses surrounding an expression.
///
/// E.g. for `((f()))` with `f()` as expression, the iterator returns the ranges (1, 6) and (0, 7).
///
/// Note that without a parent the range can be inaccurate, e.g. `f(a)` we falsely return a set of
/// parentheses around `a` even if the parentheses actually belong to `f`. That is why you should
/// generally prefer [`parenthesized_range_from_tokens`].
pub fn parentheses_iterator_from_tokens<'a>(
    expr: ExprRef<'a>,
    parent: Option<AnyNodeRef>,
    tokens: &'a [Token],
) -> impl Iterator<Item = TextRange> + 'a {
    let exclusive_parent_end = if let Some(parent) = parent {
        // If the parent is a node that brings its own parentheses, exclude the closing parenthesis
        // from our search range. Otherwise, we risk matching on calls, like `func(x)`, for which
        // the open and close parentheses are part of the `Arguments` node.
        if parent.is_arguments() {
            parent.end() - ")".text_len()
        } else {
            parent.end()
        }
    } else {
        tokens.last().map(|t| t.end()).unwrap_or(expr.end())
    };

    let after_expr_idx = tokens.partition_point(|token| token.end() <= expr.end());
    let tokens_after = &tokens[after_expr_idx..];

    let before_expr_idx = tokens.partition_point(|token| token.start() < expr.start());
    let tokens_before = &tokens[..before_expr_idx];

    let right_parens = tokens_after
        .iter()
        .filter(move |token| token.start() < exclusive_parent_end)
        .filter(|token| !is_trivia(token.kind()))
        .take_while(|token| token.kind() == TokenKind::Rpar);

    let left_parens = tokens_before
        .iter()
        .rev()
        .filter(|token| !is_trivia(token.kind()))
        .take_while(|token| token.kind() == TokenKind::Lpar);

    // Zip closing parenthesis with opening parenthesis. The order is intentional, as testing for
    // closing parentheses is cheaper, and `zip` will avoid progressing the `left_parens` if
    // the `right_parens` is exhausted.
    right_parens
        .zip(left_parens)
        .map(|(right, left)| TextRange::new(left.start(), right.end()))
}

/// Returns the [`TextRange`] of a given expression including parentheses, if the expression is
/// parenthesized; or `None`, if the expression is not parenthesized.
pub fn parenthesized_range_from_tokens(
    expr: ExprRef,
    parent: AnyNodeRef,
    tokens: &[Token],
) -> Option<TextRange> {
    parentheses_iterator_from_tokens(expr, Some(parent), tokens).last()
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::{AnyNodeRef, Expr, Stmt};
    use ruff_text_size::TextRange;

    use crate::{parenthesized_range_from_tokens, parse_module};

    #[test]
    fn test_parenthesized_range() {
        let source = "x = (1 + 2)";
        let parsed = parse_module(source).unwrap();
        let tokens = parsed.tokens();
        let stmt = parsed.suite().first().unwrap();

        let Stmt::Assign(assign) = stmt else {
            panic!("Expected Assign statement");
        };

        let value = assign.value.as_ref();
        let range =
            parenthesized_range_from_tokens(value.into(), AnyNodeRef::from(stmt), tokens.as_ref());

        assert_eq!(range, Some(TextRange::new(4.into(), 11.into())));
    }

    #[test]
    fn test_double_parenthesized_range() {
        let source = "x = ((1 + 2))";
        let parsed = parse_module(source).unwrap();
        let tokens = parsed.tokens();
        let stmt = parsed.suite().first().unwrap();

        let Stmt::Assign(assign) = stmt else {
            panic!("Expected Assign statement");
        };

        let value = assign.value.as_ref();
        let range =
            parenthesized_range_from_tokens(value.into(), AnyNodeRef::from(stmt), tokens.as_ref());

        // Should return the outermost parentheses
        assert_eq!(range, Some(TextRange::new(4.into(), 13.into())));
    }

    #[test]
    fn test_no_parentheses() {
        let source = "x = 1 + 2";
        let parsed = parse_module(source).unwrap();
        let tokens = parsed.tokens();
        let stmt = parsed.suite().first().unwrap();

        let Stmt::Assign(assign) = stmt else {
            panic!("Expected Assign statement");
        };

        let value = assign.value.as_ref();
        let range =
            parenthesized_range_from_tokens(value.into(), AnyNodeRef::from(stmt), tokens.as_ref());

        assert_eq!(range, None);
    }

    #[test]
    fn test_call_parentheses_not_included() {
        let source = "f(a)";
        let parsed = parse_module(source).unwrap();
        let tokens = parsed.tokens();
        let stmt = parsed.suite().first().unwrap();

        let Stmt::Expr(expr_stmt) = stmt else {
            panic!("Expected Expr statement");
        };

        let Expr::Call(call) = expr_stmt.value.as_ref() else {
            panic!("Expected Call expression");
        };

        // Get the argument `a`
        let arg = call.arguments.args.first().unwrap();
        let range = parenthesized_range_from_tokens(
            arg.into(),
            AnyNodeRef::from(&call.arguments),
            tokens.as_ref(),
        );

        // `a` is not parenthesized, the parens belong to the call
        assert_eq!(range, None);
    }

    #[test]
    fn test_parenthesized_arg_in_call() {
        let source = "f((a))";
        let parsed = parse_module(source).unwrap();
        let tokens = parsed.tokens();
        let stmt = parsed.suite().first().unwrap();

        let Stmt::Expr(expr_stmt) = stmt else {
            panic!("Expected Expr statement");
        };

        let Expr::Call(call) = expr_stmt.value.as_ref() else {
            panic!("Expected Call expression");
        };

        // Get the argument `a`
        let arg = call.arguments.args.first().unwrap();
        let range = parenthesized_range_from_tokens(
            arg.into(),
            AnyNodeRef::from(&call.arguments),
            tokens.as_ref(),
        );

        // `a` is parenthesized within the call
        assert_eq!(range, Some(TextRange::new(2.into(), 5.into())));
    }
}
