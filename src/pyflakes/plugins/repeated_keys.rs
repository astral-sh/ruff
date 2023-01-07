use std::hash::{BuildHasherDefault, Hash};

use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind};

use crate::ast::comparable::{ComparableConstant, ComparableExpr};
use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode};
use crate::violations;

#[derive(Debug, Eq, PartialEq, Hash)]
enum DictionaryKey<'a> {
    Constant(ComparableConstant<'a>),
    Variable(&'a str),
}

fn into_dictionary_key(expr: &Expr) -> Option<DictionaryKey> {
    match &expr.node {
        ExprKind::Constant { value, .. } => Some(DictionaryKey::Constant(value.into())),
        ExprKind::Name { id, .. } => Some(DictionaryKey::Variable(id)),
        _ => None,
    }
}

/// F601, F602
pub fn repeated_keys(checker: &mut Checker, keys: &[Expr], values: &[Expr]) {
    // Generate a map from key to (index, value).
    let mut seen: FxHashMap<DictionaryKey, FxHashSet<ComparableExpr>> =
        FxHashMap::with_capacity_and_hasher(keys.len(), BuildHasherDefault::default());

    // Detect duplicate keys.
    for (i, key) in keys.iter().enumerate() {
        if let Some(key) = into_dictionary_key(key) {
            if let Some(seen_values) = seen.get_mut(&key) {
                match key {
                    DictionaryKey::Constant(..) => {
                        if checker.settings.enabled.contains(&CheckCode::F601) {
                            let comparable_value: ComparableExpr = (&values[i]).into();
                            let is_duplicate_value = seen_values.contains(&comparable_value);
                            let mut check = Check::new(
                                violations::MultiValueRepeatedKeyLiteral(
                                    unparse_expr(&keys[i], checker.style),
                                    is_duplicate_value,
                                ),
                                Range::from_located(&keys[i]),
                            );
                            if is_duplicate_value {
                                if checker.patch(&CheckCode::F601) {
                                    check.amend(Fix::deletion(
                                        values[i - 1].end_location.unwrap(),
                                        values[i].end_location.unwrap(),
                                    ));
                                }
                            } else {
                                seen_values.insert(comparable_value);
                            }
                            checker.checks.push(check);
                        }
                    }
                    DictionaryKey::Variable(key) => {
                        if checker.settings.enabled.contains(&CheckCode::F602) {
                            let comparable_value: ComparableExpr = (&values[i]).into();
                            let is_duplicate_value = seen_values.contains(&comparable_value);
                            let mut check = Check::new(
                                violations::MultiValueRepeatedKeyVariable(
                                    key.to_string(),
                                    is_duplicate_value,
                                ),
                                Range::from_located(&keys[i]),
                            );
                            if is_duplicate_value {
                                if checker.patch(&CheckCode::F602) {
                                    check.amend(Fix::deletion(
                                        values[i - 1].end_location.unwrap(),
                                        values[i].end_location.unwrap(),
                                    ));
                                }
                            } else {
                                seen_values.insert(comparable_value);
                            }
                            checker.checks.push(check);
                        }
                    }
                }
            } else {
                seen.insert(key, FxHashSet::from_iter([(&values[i]).into()]));
            }
        }
    }
}
