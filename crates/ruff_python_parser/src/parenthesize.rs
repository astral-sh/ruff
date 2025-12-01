use ruff_python_ast::{AnyNodeRef, ExprRef};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::{TokenKind, Tokens};

/// Tokens that should be treated as trivia when scanning around parentheses.
/// Mirrors the behavior of `SimpleTokenKind::is_trivia()` as closely as possible
/// at the `TokenKind` level.
const fn is_trivia(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Comment
            | TokenKind::Newline
            | TokenKind::NonLogicalNewline
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
/// generally prefer [`parenthesized_range`].
pub fn parentheses_iterator<'a>(
    expr: ExprRef<'a>,
    parent: Option<AnyNodeRef>,
    tokens: &'a Tokens,
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
        tokens.last().map_or(expr.end(), |t| t.end())
    };

    let right_parens = tokens
        .after(expr.end())
        .iter()
        .take_while(move |token| token.start() < exclusive_parent_end)
        .filter(|token| !is_trivia(token.kind()))
        .take_while(|token| token.kind() == TokenKind::Rpar);

    let left_parens = tokens
        .before(expr.start())
        .iter()
        .rev()
        .filter(|token| !is_trivia(token.kind()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_module;
    use ruff_python_ast::{self as ast, Expr};

    #[test]
    fn test_no_parentheses() {
        let source = "x = 2 + 2";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        assert_eq!(result, None);
    }

    #[test]
    fn test_single_parentheses() {
        let source = "x = (2 + 2)";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        let range = result.expect("should find parentheses");
        assert_eq!(&source[range], "(2 + 2)");
    }

    #[test]
    fn test_double_parentheses() {
        let source = "x = ((2 + 2))";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        let range = result.expect("should find parentheses");
        assert_eq!(&source[range], "((2 + 2))");
    }

    #[test]
    fn test_parentheses_with_whitespace() {
        let source = "x = (  2 + 2  )";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        let range = result.expect("should find parentheses");
        assert_eq!(&source[range], "(  2 + 2  )");
    }

    #[test]
    fn test_parentheses_with_comments() {
        let source = "x = ( # comment\n    2 + 2\n)";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        let range = result.expect("should find parentheses");
        assert_eq!(&source[range], "( # comment\n    2 + 2\n)");
    }

    #[test]
    fn test_parenthesized_range_multiple() {
        let source = "x = (((2 + 2)))";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        let range = result.expect("should find parentheses");
        assert_eq!(&source[range], "(((2 + 2)))");
    }

    #[test]
    fn test_parentheses_iterator_multiple() {
        let source = "x = (((2 + 2)))";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let ranges: Vec<_> =
            parentheses_iterator(assign.value.as_ref().into(), Some(stmt.into()), tokens).collect();
        assert_eq!(ranges.len(), 3);
        assert_eq!(&source[ranges[0]], "(2 + 2)");
        assert_eq!(&source[ranges[1]], "((2 + 2))");
        assert_eq!(&source[ranges[2]], "(((2 + 2)))");
    }

    #[test]
    fn test_call_arguments_not_counted() {
        let source = "f(x)";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Expr(expr_stmt) = stmt else {
            panic!("expected `Expr` statement, got {stmt:?}");
        };

        let Expr::Call(call) = expr_stmt.value.as_ref() else {
            panic!("expected Call expression, got {:?}", expr_stmt.value);
        };

        let arg = call
            .arguments
            .args
            .first()
            .expect("call should have an argument");
        let result = parenthesized_range(arg.into(), (&call.arguments).into(), tokens);
        // The parentheses belong to the call, not the argument
        assert_eq!(result, None);
    }

    #[test]
    fn test_call_with_parenthesized_argument() {
        let source = "f((x))";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Expr(expr_stmt) = stmt else {
            panic!("expected Expr statement, got {stmt:?}");
        };

        let Expr::Call(call) = expr_stmt.value.as_ref() else {
            panic!("expected `Call` expression, got {:?}", expr_stmt.value);
        };

        let arg = call
            .arguments
            .args
            .first()
            .expect("call should have an argument");
        let result = parenthesized_range(arg.into(), (&call.arguments).into(), tokens);

        let range = result.expect("should find parentheses around argument");
        assert_eq!(&source[range], "(x)");
    }

    #[test]
    fn test_multiline_with_parentheses() {
        let source = "x = (\n    2 + 2 + 2\n)";
        let parsed = parse_module(source).expect("should parse valid python");
        let tokens = parsed.tokens();
        let module = parsed.syntax();

        let stmt = module.body.first().expect("module should have a statement");
        let ast::Stmt::Assign(assign) = stmt else {
            panic!("expected `Assign` statement, got {stmt:?}");
        };

        let result = parenthesized_range(assign.value.as_ref().into(), stmt.into(), tokens);
        let range = result.expect("should find parentheses");
        assert_eq!(&source[range], "(\n    2 + 2 + 2\n)");
    }
}
