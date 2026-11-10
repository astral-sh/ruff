use std::collections::HashSet;
use std::hash::{BuildHasher, RandomState};

use ruff_python_ast::comparable::{
    ComparableExpr, ComparableLiteral, ComparablePattern, ComparableStmt, HashableExpr,
};
use ruff_python_ast::{HasNodeIndex, NodeIndex};
use ruff_python_parser::{ParseError, parse_expression, parse_module};

#[track_caller]
fn assert_comparable(left: &str, right: &str) -> Result<(), ParseError> {
    let left_parsed = parse_expression(left)?;
    let right_parsed = parse_expression(right)?;

    let left_compr = ComparableExpr::from(left_parsed.expr());
    let right_compr = ComparableExpr::from(right_parsed.expr());

    assert_eq!(left_compr, right_compr);
    assert!(HashSet::from([left_compr]).contains(&right_compr));
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

    let left = HashableExpr::from(left_parsed.expr());
    let right = HashableExpr::from(right_parsed.expr());
    assert_eq!(left, right);
    assert!(HashSet::from([left]).contains(&right));

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
fn call_arguments() -> Result<(), ParseError> {
    let call = "factory(item, *items, limit=2, **options)";
    assert_comparable(call, "factory( (item), *items, limit=0x2, **options, )")?;
    assert_noncomparable(call, "factory(*items, item, limit=2, **options)")?;
    assert_noncomparable(call, "factory(item, *items, limit=3, **options)")?;
    assert_noncomparable(call, "factory(item, *items, limit=2, options=options)")?;
    assert_noncomparable(call, "factory(item, *items, **options, limit=2)")
}

#[test]
fn names_ignore_context_and_location() -> Result<(), ParseError> {
    let parsed = parse_expression("[item for item in items]")?;
    let comprehension = parsed.expr().as_list_comp_expr().unwrap();
    let target = &comprehension.generators[0].target;
    let name = target.as_name_expr().unwrap();

    let seen = HashSet::from([ComparableExpr::from(&comprehension.elt)]);
    assert!(seen.contains(&ComparableExpr::from(target)));
    assert!(seen.contains(&ComparableExpr::from(name)));
    Ok(())
}

#[test]
fn assigned_node_index_preserves_hash_lookup() -> Result<(), ParseError> {
    let parsed = parse_expression("name")?;
    let other = parse_expression("name")?;
    let key = ComparableExpr::from(parsed.expr());
    let seen = HashSet::from([key]);
    let hash = seen.hasher().hash_one(key);

    parsed.expr().node_index().set(NodeIndex::from(7));
    assert_eq!(seen.hasher().hash_one(key), hash);
    assert!(seen.contains(&ComparableExpr::from(other.expr())));
    Ok(())
}

#[test]
fn compound_statements() -> Result<(), ParseError> {
    let left = parse_module("if ready:\n    send('a' 'b')\nelif retry:\n    pass")?;
    let right = parse_module("if (ready): send('ab')\nelif retry: pass")?;
    let different_test = parse_module("if ready: send('ab')\nelif later: pass")?;
    let different_body = parse_module("if ready: send('ab')\nelif retry: send('ab')")?;

    let left = ComparableStmt::from(&left.syntax().body[0]);
    let right = ComparableStmt::from(&right.syntax().body[0]);
    assert_eq!(left, right);
    let hasher = RandomState::new();
    assert_eq!(hasher.hash_one(left), hasher.hash_one(right));
    for different in [&different_test, &different_body] {
        assert_ne!(left, ComparableStmt::from(&different.syntax().body[0]));
    }
    Ok(())
}

#[test]
fn match_patterns() -> Result<(), ParseError> {
    fn pattern(stmt: &ruff_python_ast::Stmt) -> ComparablePattern<'_> {
        ComparablePattern::from(&stmt.as_match_stmt().unwrap().cases[0].pattern)
    }

    let left = parse_module("match x:\n    case Point(x=[1, *rest]): pass")?;
    let right = parse_module("match other:\n    case Point(x = [0x1, *rest]): pass")?;
    let different = parse_module("match x:\n    case Point(y=[1, *rest]): pass")?;
    let left = pattern(&left.syntax().body[0]);
    let right = pattern(&right.syntax().body[0]);
    assert_eq!(left, right);
    let hasher = RandomState::new();
    assert_eq!(hasher.hash_one(left), hasher.hash_one(right));
    assert_ne!(left, pattern(&different.syntax().body[0]));
    Ok(())
}

#[test]
fn literal_keys_preserve_part_boundaries() -> Result<(), ParseError> {
    fn key(expr: &ruff_python_ast::Expr) -> ComparableLiteral<'_> {
        ComparableLiteral::from(expr.as_literal_expr().unwrap())
    }

    let split = parse_expression("'a' 'b'")?;
    let equal = parse_expression("('a'  'b')")?;
    let joined = parse_expression("'ab'")?;
    let seen = HashSet::from([key(split.expr())]);
    assert!(seen.contains(&key(equal.expr())));
    assert!(!seen.contains(&key(joined.expr())));
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
    assert_hashable_equal("True", "1.0")?;
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
