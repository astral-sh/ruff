use ruff_python_ast::parenthesize as original_parenthesize;
use ruff_python_parser::parenthesize as optimized_parenthesize;
use ruff_python_parser::parse_expression;
use ruff_python_trivia::CommentRanges;

#[test]
fn test_optimized_vs_original_parenthesized_name() {
    let source_code = r"(x) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let original = original_parenthesize::parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(name.into(), bin_op.into(), parsed.tokens());

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "(x)");
}

#[test]
fn test_optimized_vs_original_non_parenthesized_name() {
    let source_code = r"x + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let original = original_parenthesize::parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(name.into(), bin_op.into(), parsed.tokens());

    assert_eq!(original, optimized);
    assert_eq!(optimized, None);
}

#[test]
fn test_optimized_vs_original_parenthesized_argument() {
    let source_code = r"f((a))";
    let parsed = parse_expression(source_code).unwrap();

    let call = parsed.expr().as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let original = original_parenthesize::parenthesized_range(
        argument.into(),
        arguments.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized = optimized_parenthesize::parenthesized_range(
        argument.into(),
        arguments.into(),
        parsed.tokens(),
    );

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "(a)");
}

#[test]
fn test_optimized_vs_original_non_parenthesized_argument() {
    let source_code = r"f(a)";
    let parsed = parse_expression(source_code).unwrap();

    let call = parsed.expr().as_call_expr().unwrap();
    let arguments = &call.arguments;
    let argument = arguments.args.first().unwrap();

    let original = original_parenthesize::parenthesized_range(
        argument.into(),
        arguments.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized = optimized_parenthesize::parenthesized_range(
        argument.into(),
        arguments.into(),
        parsed.tokens(),
    );

    assert_eq!(original, optimized);
    assert_eq!(optimized, None);
}

#[test]
fn test_optimized_vs_original_twice_parenthesized() {
    let source_code = r"((x)) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let original = original_parenthesize::parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(name.into(), bin_op.into(), parsed.tokens());

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "((x))");
}

#[test]
fn test_optimized_vs_original_with_whitespace() {
    let source_code = r"(  x  ) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let original = original_parenthesize::parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(name.into(), bin_op.into(), parsed.tokens());

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "(  x  )");
}

#[test]
fn test_optimized_vs_original_with_comments() {
    let source_code = r"( # comment
    x
) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let comment_ranges = CommentRanges::from(parsed.tokens());

    let original = original_parenthesize::parenthesized_range(
        name.into(),
        bin_op.into(),
        &comment_ranges,
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(name.into(), bin_op.into(), parsed.tokens());

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "( # comment\n    x\n)");
}

#[test]
fn test_optimized_vs_original_multiple_layers() {
    let source_code = r"(((x))) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let original = original_parenthesize::parenthesized_range(
        name.into(),
        bin_op.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(name.into(), bin_op.into(), parsed.tokens());

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "(((x)))");
}

#[test]
fn test_optimized_vs_original_iterator_all_layers() {
    let source_code = r"(((x))) + 1";
    let parsed = parse_expression(source_code).unwrap();

    let bin_op = parsed.expr().as_bin_op_expr().unwrap();
    let name = bin_op.left.as_ref();

    let original_layers: Vec<_> = original_parenthesize::parentheses_iterator(
        name.into(),
        Some(bin_op.into()),
        &CommentRanges::default(),
        source_code,
    )
    .collect();

    let optimized_layers: Vec<_> = optimized_parenthesize::parentheses_iterator(
        name.into(),
        Some(bin_op.into()),
        parsed.tokens(),
    )
    .collect();

    assert_eq!(original_layers, optimized_layers);
    assert_eq!(optimized_layers.len(), 3);
    assert_eq!(&source_code[optimized_layers[0]], "(x)");
    assert_eq!(&source_code[optimized_layers[1]], "((x))");
    assert_eq!(&source_code[optimized_layers[2]], "(((x)))");
}

#[test]
fn test_optimized_vs_original_complex_expression() {
    let source_code = r"((a + b) * (c - d))";
    let parsed = parse_expression(source_code).unwrap();

    let outer_paren = parsed.expr().as_bin_op_expr().unwrap();

    let original = original_parenthesize::parenthesized_range(
        outer_paren.into(),
        outer_paren.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized = optimized_parenthesize::parenthesized_range(
        outer_paren.into(),
        outer_paren.into(),
        parsed.tokens(),
    );

    assert_eq!(original, optimized);
}

#[test]
fn test_optimized_vs_original_tuple_member() {
    let source_code = r"(a, (b))";
    let parsed = parse_expression(source_code).unwrap();

    let tuple = parsed.expr().as_tuple_expr().unwrap();
    let member = tuple.elts.last().unwrap();

    let original = original_parenthesize::parenthesized_range(
        member.into(),
        tuple.into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized =
        optimized_parenthesize::parenthesized_range(member.into(), tuple.into(), parsed.tokens());

    assert_eq!(original, optimized);
    let range = optimized.expect("should find parentheses");
    assert_eq!(&source_code[range], "(b)");
}

#[test]
fn test_optimized_vs_original_nested_calls() {
    let source_code = r"f(g((h(x))))";
    let parsed = parse_expression(source_code).unwrap();

    let outer_call = parsed.expr().as_call_expr().unwrap();
    let inner_arg = outer_call.arguments.args.first().unwrap();

    let original = original_parenthesize::parenthesized_range(
        inner_arg.into(),
        (&outer_call.arguments).into(),
        &CommentRanges::default(),
        source_code,
    );

    let optimized = optimized_parenthesize::parenthesized_range(
        inner_arg.into(),
        (&outer_call.arguments).into(),
        parsed.tokens(),
    );

    assert_eq!(original, optimized);
}
