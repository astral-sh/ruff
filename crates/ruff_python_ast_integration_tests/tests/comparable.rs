use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_parser::{parse_expression, ParseError};

#[test]
fn concatenated_strings_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"'a' 'b' r'\n raw'"#;
    let value_contents = r#"'ab\\n raw'"#;

    let split_parsed = parse_expression(split_contents)?;
    let value_parsed = parse_expression(value_contents)?;

    let split_compr = ComparableExpr::from(split_parsed.expr());
    let value_compr = ComparableExpr::from(value_parsed.expr());

    assert_eq!(split_compr, value_compr);
    Ok(())
}

#[test]
fn concatenated_bytes_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"b'a' b'b'"#;
    let value_contents = r#"b'ab'"#;

    let split_parsed = parse_expression(split_contents)?;
    let value_parsed = parse_expression(value_contents)?;

    let split_compr = ComparableExpr::from(split_parsed.expr());
    let value_compr = ComparableExpr::from(value_parsed.expr());

    assert_eq!(split_compr, value_compr);
    Ok(())
}

#[test]
fn concatenated_fstrings_compare_equal() -> Result<(), ParseError> {
    let split_contents = r#"f"{foo!r} this" r"\n raw" f" and {bar!s} that""#;
    let value_contents = r#"f"{foo!r} this\\n raw and {bar!s} that""#;

    let split_parsed = parse_expression(split_contents)?;
    let value_parsed = parse_expression(value_contents)?;

    let split_compr = ComparableExpr::from(split_parsed.expr());
    let value_compr = ComparableExpr::from(value_parsed.expr());

    assert_eq!(split_compr, value_compr);
    Ok(())
}
