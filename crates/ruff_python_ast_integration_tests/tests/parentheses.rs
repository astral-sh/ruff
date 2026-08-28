//! Tests for [`ruff_python_ast::token::parentheses_iterator`],
//! [`ruff_python_ast::token::parenthesized_range`], and
//! [`ruff_python_ast::helpers::unwrapped_call_argument`].

use std::error::Error;

use ruff_python_ast::{
    self as ast, AnyNodeRef, Expr,
    find_node::covering_node,
    helpers::unwrapped_call_argument,
    token::{parentheses_iterator, parenthesized_range},
};
use ruff_python_parser::parse_module;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

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

#[test]
fn call_unwrapping_preserves_context() -> Result<(), Box<dyn Error>> {
    for (source, expected) in [
        ("wrap(f if flag else g)()", "(f if flag else g)()"),
        (
            "[x for x in wrap(a if flag else b)]",
            "[x for x in (a if flag else b)]",
        ),
        ("f\"{wrap({})}\"", "f\"{({})}\""),
    ] {
        let parsed = parse_module(source)?;
        let start = TextSize::try_from(source.find("wrap").ok_or("missing wrap call")?)?;
        let covering = covering_node(
            parsed.syntax().into(),
            TextRange::at(start, "wrap".text_len()),
        )
        .find_first(|node| matches!(node, AnyNodeRef::ExprCall(_)))
        .map_err(|_| "missing enclosing call")?;
        let AnyNodeRef::ExprCall(call) = covering.node() else {
            return Err("expected a call expression".into());
        };
        let argument = call.arguments.args.first().ok_or("missing argument")?;
        let replacement =
            unwrapped_call_argument(call, argument, covering.parent(), parsed.tokens(), source);

        let mut fixed = source.to_string();
        fixed.replace_range(
            usize::from(call.start())..usize::from(call.end()),
            &replacement,
        );
        assert_eq!(fixed, expected, "{source}");
        parse_module(&fixed)?;
    }
    Ok(())
}
