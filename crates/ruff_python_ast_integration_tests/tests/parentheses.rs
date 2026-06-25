//! Tests for [`ruff_python_ast::tokens::parentheses_iterator`] and
//! [`ruff_python_ast::tokens::parenthesized_range`].

use ruff_python_ast::{
    self as ast, Expr,
    token::{parentheses_iterator, parenthesized_range},
};
use ruff_python_parser::parse_module;

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
