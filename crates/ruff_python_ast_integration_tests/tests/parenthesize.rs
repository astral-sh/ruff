use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_parser::parse_expression;
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;

#[test]
fn test_parenthesized_name() {
    let source_code = r"(x) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let parenthesized = parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, Some(TextRange::new(0.into(), 3.into())));
}

#[test]
fn test_non_parenthesized_name() {
    let source_code = r"x + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let parenthesized = parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, None);
}

#[test]
fn test_parenthesized_argument() {
    let source_code = r"f((a))";
    let parsed = parse_expression(source_code).unwrap();

    let call = parsed.expr().as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let parenthesized = parenthesized_range(
        argument.into(),
        arguments.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, Some(TextRange::new(2.into(), 5.into())));
}

#[test]
fn test_non_parenthesized_argument() {
    let source_code = r"f(a)";
    let parsed = parse_expression(source_code).unwrap();

    let call = parsed.expr().as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let parenthesized = parenthesized_range(
        argument.into(),
        arguments.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, None);
}

#[test]
fn test_parenthesized_tuple_member() {
    let source_code = r"(a, (b))";
    let parsed = parse_expression(source_code).unwrap();

    let tuple = parsed.expr().as_tuple_expr().unwrap();
    let member = tuple.elts.last().unwrap();

    let parenthesized = parenthesized_range(
        member.into(),
        tuple.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, Some(TextRange::new(4.into(), 7.into())));
}

#[test]
fn test_non_parenthesized_tuple_member() {
    let source_code = r"(a, b)";
    let parsed = parse_expression(source_code).unwrap();

    let tuple = parsed.expr().as_tuple_expr().unwrap();
    let member = tuple.elts.last().unwrap();

    let parenthesized = parenthesized_range(
        member.into(),
        tuple.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, None);
}

#[test]
fn test_twice_parenthesized_name() {
    let source_code = r"((x)) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let parenthesized = parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, Some(TextRange::new(0.into(), 5.into())));
}

#[test]
fn test_twice_parenthesized_argument() {
    let source_code = r"f(((a + 1)))";
    let parsed = parse_expression(source_code).unwrap();

    let call = parsed.expr().as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let parenthesized = parenthesized_range(
        argument.into(),
        arguments.into(),
        &CommentRanges::default(),
        source_code,
    );
    assert_eq!(parenthesized, Some(TextRange::new(2.into(), 11.into())));
}
