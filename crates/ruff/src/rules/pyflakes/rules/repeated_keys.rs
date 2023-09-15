use std::hash::BuildHasherDefault;

use ruff_python_ast::Expr;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::autofix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_text_size::Ranged;

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
    name: SourceCodeSnippet,
}

impl Violation for MultiValueRepeatedKeyLiteral {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let MultiValueRepeatedKeyLiteral { name } = self;
        if let Some(name) = name.full_display() {
            format!("Dictionary key literal `{name}` repeated")
        } else {
            format!("Dictionary key literal repeated")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let MultiValueRepeatedKeyLiteral { name } = self;
        if let Some(name) = name.full_display() {
            Some(format!("Remove repeated key literal `{name}`"))
        } else {
            Some(format!("Remove repeated key literal"))
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
    name: SourceCodeSnippet,
}

impl Violation for MultiValueRepeatedKeyVariable {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let MultiValueRepeatedKeyVariable { name } = self;
        if let Some(name) = name.full_display() {
            format!("Dictionary key `{name}` repeated")
        } else {
            format!("Dictionary key repeated")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let MultiValueRepeatedKeyVariable { name } = self;
        if let Some(name) = name.full_display() {
            Some(format!("Remove repeated key `{name}`"))
        } else {
            Some(format!("Remove repeated key"))
        }
    }
}

/// F601, F602
pub(crate) fn repeated_keys(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    // Generate a map from key to (index, value).
    let mut seen: FxHashMap<ComparableExpr, FxHashSet<ComparableExpr>> =
        FxHashMap::with_capacity_and_hasher(keys.len(), BuildHasherDefault::default());

    // Detect duplicate keys.
    for (i, key) in keys.iter().enumerate() {
        let Some(key) = key else {
            continue;
        };

        let comparable_key = ComparableExpr::from(key);
        let comparable_value = ComparableExpr::from(&values[i]);

        let Some(seen_values) = seen.get_mut(&comparable_key) else {
            seen.insert(comparable_key, FxHashSet::from_iter([comparable_value]));
            continue;
        };

        match key {
            Expr::Constant(_) | Expr::Tuple(_) | Expr::FString(_) => {
                if checker.enabled(Rule::MultiValueRepeatedKeyLiteral) {
                    let mut diagnostic = Diagnostic::new(
                        MultiValueRepeatedKeyLiteral {
                            name: SourceCodeSnippet::from_str(checker.locator().slice(key)),
                        },
                        key.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        if !seen_values.insert(comparable_value) {
                            diagnostic.set_fix(Fix::suggested(Edit::deletion(
                                values[i - 1].end(),
                                values[i].end(),
                            )));
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
            Expr::Name(_) => {
                if checker.enabled(Rule::MultiValueRepeatedKeyVariable) {
                    let mut diagnostic = Diagnostic::new(
                        MultiValueRepeatedKeyVariable {
                            name: SourceCodeSnippet::from_str(checker.locator().slice(key)),
                        },
                        key.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        let comparable_value: ComparableExpr = (&values[i]).into();
                        if !seen_values.insert(comparable_value) {
                            diagnostic.set_fix(Fix::suggested(Edit::deletion(
                                values[i - 1].end(),
                                values[i].end(),
                            )));
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
            _ => {}
        }
    }
}
