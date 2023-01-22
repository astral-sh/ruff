use std::hash::{BuildHasherDefault, Hash};

use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind};

use crate::ast::comparable::{ComparableConstant, ComparableExpr};
use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
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
pub fn repeated_keys(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    // Generate a map from key to (index, value).
    let mut seen: FxHashMap<DictionaryKey, FxHashSet<ComparableExpr>> =
        FxHashMap::with_capacity_and_hasher(keys.len(), BuildHasherDefault::default());

    // Detect duplicate keys.
    for (i, key) in keys.iter().enumerate() {
        let Some(key) = key else {
            continue;
        };
        if let Some(dict_key) = into_dictionary_key(key) {
            if let Some(seen_values) = seen.get_mut(&dict_key) {
                match dict_key {
                    DictionaryKey::Constant(..) => {
                        if checker
                            .settings
                            .rules
                            .enabled(&Rule::MultiValueRepeatedKeyLiteral)
                        {
                            let comparable_value: ComparableExpr = (&values[i]).into();
                            let is_duplicate_value = seen_values.contains(&comparable_value);
                            let mut diagnostic = Diagnostic::new(
                                violations::MultiValueRepeatedKeyLiteral(
                                    unparse_expr(key, checker.stylist),
                                    is_duplicate_value,
                                ),
                                Range::from_located(key),
                            );
                            if is_duplicate_value {
                                if checker.patch(&Rule::MultiValueRepeatedKeyLiteral) {
                                    diagnostic.amend(Fix::deletion(
                                        values[i - 1].end_location.unwrap(),
                                        values[i].end_location.unwrap(),
                                    ));
                                }
                            } else {
                                seen_values.insert(comparable_value);
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    DictionaryKey::Variable(dict_key) => {
                        if checker
                            .settings
                            .rules
                            .enabled(&Rule::MultiValueRepeatedKeyVariable)
                        {
                            let comparable_value: ComparableExpr = (&values[i]).into();
                            let is_duplicate_value = seen_values.contains(&comparable_value);
                            let mut diagnostic = Diagnostic::new(
                                violations::MultiValueRepeatedKeyVariable(
                                    dict_key.to_string(),
                                    is_duplicate_value,
                                ),
                                Range::from_located(key),
                            );
                            if is_duplicate_value {
                                if checker.patch(&Rule::MultiValueRepeatedKeyVariable) {
                                    diagnostic.amend(Fix::deletion(
                                        values[i - 1].end_location.unwrap(),
                                        values[i].end_location.unwrap(),
                                    ));
                                }
                            } else {
                                seen_values.insert(comparable_value);
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
            } else {
                seen.insert(dict_key, FxHashSet::from_iter([(&values[i]).into()]));
            }
        }
    }
}
