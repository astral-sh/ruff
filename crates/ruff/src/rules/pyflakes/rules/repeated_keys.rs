use std::hash::{BuildHasherDefault, Hash};

use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::{ComparableConstant, ComparableExpr};

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

/// ## What it does
/// Checks for dictionary literals that associate multiple values with the
/// same key.
///
/// ## Why is this bad?
/// Dictionary keys should be unique. If a key is associated with multiple values,
/// the earlier values will be overwritten. Including multiple values for the
/// same key in a dictionary literal is likely a mistake.
///
/// ## Example
/// ```python
/// foo = {
///     "bar": 1,
///     "baz": 2,
///     "baz": 3,
/// }
/// foo["baz"]  # 3
/// ```
///
/// Use instead:
/// ```python
/// foo = {
///     "bar": 1,
///     "baz": 2,
/// }
/// foo["baz"]  # 2
/// ```
///
/// ## References
/// - [Python documentation: Dictionaries](https://docs.python.org/3/tutorial/datastructures.html#dictionaries)
#[violation]
pub struct MultiValueRepeatedKeyLiteral {
    name: String,
    repeated_value: bool,
}

impl Violation for MultiValueRepeatedKeyLiteral {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let MultiValueRepeatedKeyLiteral { name, .. } = self;
        format!("Dictionary key literal `{name}` repeated")
    }

    fn autofix_title(&self) -> Option<String> {
        let MultiValueRepeatedKeyLiteral {
            repeated_value,
            name,
        } = self;
        if *repeated_value {
            Some(format!("Remove repeated key literal `{name}`"))
        } else {
            None
        }
    }
}

/// ## What it does
/// Checks for dictionary keys that are repeated with different values.
///
/// ## Why is this bad?
/// Dictionary keys should be unique. If a key is repeated with a different
/// value, the first values will be overwritten and the key will correspond to
/// the last value. This is likely a mistake.
///
/// ## Example
/// ```python
/// foo = {
///     bar: 1,
///     baz: 2,
///     baz: 3,
/// }
/// foo[baz]  # 3
/// ```
///
/// Use instead:
/// ```python
/// foo = {
///     bar: 1,
///     baz: 2,
/// }
/// foo[baz]  # 2
/// ```
///
/// ## References
/// - [Python documentation: Dictionaries](https://docs.python.org/3/tutorial/datastructures.html#dictionaries)
#[violation]
pub struct MultiValueRepeatedKeyVariable {
    name: String,
    repeated_value: bool,
}

impl Violation for MultiValueRepeatedKeyVariable {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let MultiValueRepeatedKeyVariable { name, .. } = self;
        format!("Dictionary key `{name}` repeated")
    }

    fn autofix_title(&self) -> Option<String> {
        let MultiValueRepeatedKeyVariable {
            repeated_value,
            name,
        } = self;
        if *repeated_value {
            Some(format!("Remove repeated key `{name}`"))
        } else {
            None
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
enum DictionaryKey<'a> {
    Constant(ComparableConstant<'a>),
    Variable(&'a str),
}

fn into_dictionary_key(expr: &Expr) -> Option<DictionaryKey> {
    match expr {
        Expr::Constant(ast::ExprConstant { value, .. }) => {
            Some(DictionaryKey::Constant(value.into()))
        }
        Expr::Name(ast::ExprName { id, .. }) => Some(DictionaryKey::Variable(id)),
        _ => None,
    }
}

/// F601, F602
pub(crate) fn repeated_keys(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
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
                        if checker.enabled(Rule::MultiValueRepeatedKeyLiteral) {
                            let comparable_value: ComparableExpr = (&values[i]).into();
                            let is_duplicate_value = seen_values.contains(&comparable_value);
                            let mut diagnostic = Diagnostic::new(
                                MultiValueRepeatedKeyLiteral {
                                    name: checker.generator().expr(key),
                                    repeated_value: is_duplicate_value,
                                },
                                key.range(),
                            );
                            if is_duplicate_value {
                                if checker.patch(diagnostic.kind.rule()) {
                                    diagnostic.set_fix(Fix::suggested(Edit::deletion(
                                        values[i - 1].end(),
                                        values[i].end(),
                                    )));
                                }
                            } else {
                                seen_values.insert(comparable_value);
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    DictionaryKey::Variable(dict_key) => {
                        if checker.enabled(Rule::MultiValueRepeatedKeyVariable) {
                            let comparable_value: ComparableExpr = (&values[i]).into();
                            let is_duplicate_value = seen_values.contains(&comparable_value);
                            let mut diagnostic = Diagnostic::new(
                                MultiValueRepeatedKeyVariable {
                                    name: dict_key.to_string(),
                                    repeated_value: is_duplicate_value,
                                },
                                key.range(),
                            );
                            if is_duplicate_value {
                                if checker.patch(diagnostic.kind.rule()) {
                                    diagnostic.set_fix(Fix::suggested(Edit::deletion(
                                        values[i - 1].end(),
                                        values[i].end(),
                                    )));
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
