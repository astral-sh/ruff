use ruff_python_ast::parenthesize::ParenthesizedExpression;
use ruff_python_parser::parse_expression;

#[test]
fn test_parenthesized_name() {
    let source_code = r#"(x) + 1"#;
    let expr = parse_expression(source_code, "<filename>").unwrap();

    let bin_op = expr.as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let parenthesized = ParenthesizedExpression::from_expr(name.into(), bin_op.into(), source_code);
    assert!(parenthesized.is_parenthesized());
}

#[test]
fn test_un_parenthesized_name() {
    let source_code = r#"x + 1"#;
    let expr = parse_expression(source_code, "<filename>").unwrap();

    let bin_op = expr.as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let parenthesized = ParenthesizedExpression::from_expr(name.into(), bin_op.into(), source_code);
    assert!(!parenthesized.is_parenthesized());
}

#[test]
fn test_parenthesized_argument() {
    let source_code = r#"f((a))"#;
    let expr = parse_expression(source_code, "<filename>").unwrap();

    let call = expr.as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let parenthesized =
        ParenthesizedExpression::from_expr(argument.into(), arguments.into(), source_code);
    assert!(parenthesized.is_parenthesized());
}

#[test]
fn test_unparenthesized_argument() {
    let source_code = r#"f(a)"#;
    let expr = parse_expression(source_code, "<filename>").unwrap();

    let call = expr.as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let parenthesized =
        ParenthesizedExpression::from_expr(argument.into(), arguments.into(), source_code);
    assert!(!parenthesized.is_parenthesized());
}

#[test]
fn test_parenthesized_tuple_member() {
    let source_code = r#"(a, (b))"#;
    let expr = parse_expression(source_code, "<filename>").unwrap();

    let tuple = expr.as_tuple_expr().unwrap();
    let member = tuple.elts.last().unwrap();

    let parenthesized =
        ParenthesizedExpression::from_expr(member.into(), tuple.into(), source_code);
    assert!(parenthesized.is_parenthesized());
}

#[test]
fn test_unparenthesized_tuple_member() {
    let source_code = r#"(a, b)"#;
    let expr = parse_expression(source_code, "<filename>").unwrap();

    let tuple = expr.as_tuple_expr().unwrap();
    let member = tuple.elts.last().unwrap();

    let parenthesized =
        ParenthesizedExpression::from_expr(member.into(), tuple.into(), source_code);
    assert!(!parenthesized.is_parenthesized());
}
