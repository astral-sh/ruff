use ruff_python_ast::comparable::{ComparableExpr, HashableExpr};
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

#[track_caller]
fn assert_hashable_equal(left: &str, right: &str) -> Result<(), ParseError> {
    let left_parsed = parse_expression(left)?;
    let right_parsed = parse_expression(right)?;

    assert_eq!(
        HashableExpr::from(left_parsed.expr()),
        HashableExpr::from(right_parsed.expr())
    );

    Ok(())
}

#[track_caller]
fn assert_hashable_unequal(left: &str, right: &str) -> Result<(), ParseError> {
    let left_parsed = parse_expression(left)?;
    let right_parsed = parse_expression(right)?;

    assert_ne!(
        HashableExpr::from(left_parsed.expr()),
        HashableExpr::from(right_parsed.expr())
    );

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
    let split_contents = r#"t"{foo!r} this" rt"\n raw" t" and {bar!s} that""#;
    let value_contents = r#"t"{foo!r} this\\n raw and {bar!s} that""#;

    assert_comparable(split_contents, value_contents)
}

#[test]
fn t_strings_literal_order_matters_compare_unequal() -> Result<(), ParseError> {
    let interp_then_literal_contents = r#"t"{foo}bar""#;
    let literal_then_interp_contents = r#"t"bar{foo}""#;

    assert_noncomparable(interp_then_literal_contents, literal_then_interp_contents)
}

#[test]
fn equivalent_numbers_hash_equal() -> Result<(), ParseError> {
    assert_hashable_equal("2", "2.0")?;
    assert_hashable_equal("-1", "-1.0")?;
    assert_hashable_equal("2", "2 + 0j")?;
    assert_hashable_equal("1j", "0 + 1j")?;
    assert_hashable_equal("9007199254740992", "9007199254740992.0")?;
    assert_hashable_equal("9007199254740992", "9007199254740993 + 0j")?;
    assert_hashable_equal("(2,)", "(2.0,)")?;

    assert_hashable_unequal("9007199254740993", "9007199254740992.0")?;
    assert_hashable_unequal("9007199254740993", "9007199254740993 + 0j")?;
    assert_hashable_unequal("18446744073709551615", "18446744073709551616.0")?;

    Ok(())
}

#[test]
fn large_integers_fall_back_to_structural_comparison() -> Result<(), ParseError> {
    assert_hashable_unequal("18446744073709551616", "0x10000000000000000")
}

#[test]
fn dynamic_tuple_elements_do_not_compare_by_value() -> Result<(), ParseError> {
    assert_hashable_unequal("(f(), 2)", "(f(), 2.0)")
}
