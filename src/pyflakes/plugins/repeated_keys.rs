use std::hash::{BuildHasherDefault, Hash};

use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::cmp;
use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode};
use crate::source_code_style::SourceCodeStyleDetector;
use crate::violations;

#[derive(Debug, Eq, PartialEq, Hash)]
enum DictionaryKey<'a> {
    Constant(String),
    Variable(&'a str),
}

fn into_dictionary_key<'a>(
    expr: &'a Expr,
    stylist: &SourceCodeStyleDetector,
) -> Option<DictionaryKey<'a>> {
    match &expr.node {
        ExprKind::Constant { .. } => Some(DictionaryKey::Constant(unparse_expr(expr, stylist))),
        ExprKind::Name { id, .. } => Some(DictionaryKey::Variable(id)),
        _ => None,
    }
}

/// F601, F602
pub fn repeated_keys(checker: &mut Checker, keys: &[Expr], values: &[Expr]) {
    // Generate a map from key to (index, value).
    let mut seen: FxHashMap<DictionaryKey, Vec<&Expr>> =
        FxHashMap::with_capacity_and_hasher(keys.len(), BuildHasherDefault::default());

    // Detect duplicate keys.
    for (i, key) in keys.iter().enumerate() {
        if let Some(key) = into_dictionary_key(key, checker.style) {
            if let Some(seen_values) = seen.get_mut(&key) {
                match key {
                    DictionaryKey::Constant(key) => {
                        if checker.settings.enabled.contains(&CheckCode::F601) {
                            let repeated_value =
                                seen_values.iter().any(|value| cmp::expr(value, &values[i]));
                            let mut check = Check::new(
                                violations::MultiValueRepeatedKeyLiteral(key, repeated_value),
                                Range::from_located(&keys[i]),
                            );
                            if repeated_value {
                                if checker.patch(&CheckCode::F601) {
                                    check.amend(Fix::deletion(
                                        values[i - 1].end_location.unwrap(),
                                        values[i].end_location.unwrap(),
                                    ));
                                }
                            } else {
                                seen_values.push(&values[i]);
                            }
                            checker.checks.push(check);
                        }
                    }
                    DictionaryKey::Variable(key) => {
                        if checker.settings.enabled.contains(&CheckCode::F602) {
                            let repeated_value =
                                seen_values.iter().any(|value| cmp::expr(value, &values[i]));
                            let mut check = Check::new(
                                violations::MultiValueRepeatedKeyVariable(
                                    key.to_string(),
                                    repeated_value,
                                ),
                                Range::from_located(&keys[i]),
                            );
                            if repeated_value {
                                if checker.patch(&CheckCode::F602) {
                                    check.amend(Fix::deletion(
                                        values[i - 1].end_location.unwrap(),
                                        values[i].end_location.unwrap(),
                                    ));
                                }
                            } else {
                                seen_values.push(&values[i]);
                            }
                            checker.checks.push(check);
                        }
                    }
                }
            } else {
                seen.insert(key, vec![&values[i]]);
            }
        }
    }
}
