use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_parser::{ParseError, parse_expression};

#[track_caller]
fn assert_comparable(left: &str, right: &str) -> Result<(), ParseError> {
    let left_parsed = parse_expression(left)?;
    let right_parsed = parse_expression(right)?;

    let left_compr = ComparableExpr::from(left_parsed.expr());
    let right_compr = ComparableExpr::from(right_parsed.expr());

    assert_eq!(left_compr, right_compr);
    Ok(())
}

#[track_caller]
fn assert_noncomparable(left: &str, right: &str) -> Result<(), ParseError> {
    let left_parsed = parse_expression(left)?;
    let right_parsed = parse_expression(right)?;

    let left_compr = ComparableExpr::from(left_parsed.expr());
    let right_compr = ComparableExpr::from(right_parsed.expr());

    assert_ne!(left_compr, right_compr);
    Ok(())
}

#[test]
fn concatenated_strings_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"'a' 'b' r'\n raw'"#;
    let value_contents = r#"'ab\\n raw'"#;

    assert_comparable(split_contents, value_contents)
}

#[test]
fn concatenated_bytes_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"b'a' b'b'"#;
    let value_contents = r#"b'ab'"#;

    assert_comparable(split_contents, value_contents)
}

#[test]
fn concatenated_fstrings_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"f"{foo!r} this" r"\n raw" f" and {bar!s} that""#;
    let value_contents = r#"f"{foo!r} this\\n raw and {bar!s} that""#;

    assert_comparable(split_contents, value_contents)
}

#[test]
fn concatenated_tstrings_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"t"{foo!r} this" r"\n raw" t" and {bar!s} that""#;
    let value_contents = r#"t"{foo!r} this\\n raw and {bar!s} that""#;

    assert_comparable(split_contents, value_contents)
}

#[test]
fn concatenated_f_and_t_strings_interwoven_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"f"{foo} this " t"{bar}" "baz""#;
    let value_contents = r#"f"{foo}" t" this {bar}" "baz""#;

    assert_comparable(split_contents, value_contents)
}

#[test]
fn concatenated_f_and_t_strings_compare_unequal_when_swapped() -> Result<(), ParseError> {
    let f_then_t_contents = r#"f"{foo!r} this" r"\n raw" t" and {bar!s} that""#;
    let t_then_f_contents = r#"t"{foo!r} this" r"\n raw" f" and {bar!s} that""#;

    assert_noncomparable(f_then_t_contents, t_then_f_contents)
}

#[test]
fn t_strings_literal_order_matters_compare_unequal() -> Result<(), ParseError> {
    let interp_then_literal_contents = r#"t"{foo}bar""#;
    let literal_then_interp_contents = r#"t"bar{foo}""#;

    assert_noncomparable(interp_then_literal_contents, literal_then_interp_contents)
}

#[test]
fn t_strings_empty_concat_equal() -> Result<(), ParseError> {
    let empty_literal = r#""" t"hey{foo}""#;
    let empty_f_string = r#"f""t"hey{foo}""#;

    assert_comparable(empty_literal, empty_f_string)
}
