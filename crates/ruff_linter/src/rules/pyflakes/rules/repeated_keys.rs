use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::{ComparableExpr, HashableExpr};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::registry::Rule;

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
#[derive(ViolationMetadata)]
pub(crate) struct MultiValueRepeatedKeyLiteral {
    name: SourceCodeSnippet,
    existing: SourceCodeSnippet,
}

impl Violation for MultiValueRepeatedKeyLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match (self.name.full_display(), self.existing.full_display()) {
            (Some(name), None) => {
                format!("Dictionary key literal `{name}` repeated")
            }
            (Some(name), Some(existing)) => {
                if name == existing {
                    format!("Dictionary key literal `{name}` repeated")
                } else {
                    format!(
                        "Dictionary key literal `{name}` repeated (`{name}` hashes to the same value as `{existing}`)"
                    )
                }
            }
            _ => "Dictionary key literal repeated".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = match self.name.full_display() {
            Some(name) => format!("Remove repeated key literal `{name}`"),
            None => "Remove repeated key literal".to_string(),
        };
        Some(title)
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
#[derive(ViolationMetadata)]
pub(crate) struct MultiValueRepeatedKeyVariable {
    name: SourceCodeSnippet,
}

impl Violation for MultiValueRepeatedKeyVariable {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(name) = self.name.full_display() {
            format!("Dictionary key `{name}` repeated")
        } else {
            "Dictionary key repeated".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = match self.name.full_display() {
            Some(name) => format!("Remove repeated key `{name}`"),
            None => "Remove repeated key".to_string(),
        };
        Some(title)
    }
}

/// F601, F602
pub(crate) fn repeated_keys(checker: &Checker, dict: &ast::ExprDict) {
    // Generate a map from key to (index, value).
    let mut seen: FxHashMap<HashableExpr, (&Expr, FxHashSet<ComparableExpr>)> =
        FxHashMap::with_capacity_and_hasher(dict.len(), FxBuildHasher);

    // Detect duplicate keys.
    for (i, ast::DictItem { key, value }) in dict.iter().enumerate() {
        let Some(key) = key else {
            continue;
        };

        let comparable_key = HashableExpr::from(key);
        let comparable_value = ComparableExpr::from(value);

        match seen.entry(comparable_key) {
            Entry::Vacant(entry) => {
                entry.insert((key, FxHashSet::from_iter([comparable_value])));
            }
            Entry::Occupied(mut entry) => {
                let (seen_key, seen_values) = entry.get_mut();
                match key {
                    Expr::StringLiteral(_)
                    | Expr::BytesLiteral(_)
                    | Expr::NumberLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
                    | Expr::EllipsisLiteral(_)
                    | Expr::Tuple(_)
                    | Expr::FString(_) => {
                        if checker.enabled(Rule::MultiValueRepeatedKeyLiteral) {
                            let mut diagnostic = Diagnostic::new(
                                MultiValueRepeatedKeyLiteral {
                                    name: SourceCodeSnippet::from_str(checker.locator().slice(key)),
                                    existing: SourceCodeSnippet::from_str(
                                        checker.locator().slice(*seen_key),
                                    ),
                                },
                                key.range(),
                            );
                            if !seen_values.insert(comparable_value) {
                                diagnostic.set_fix(Fix::unsafe_edit(Edit::deletion(
                                    parenthesized_range(
                                        dict.value(i - 1).into(),
                                        dict.into(),
                                        checker.comment_ranges(),
                                        checker.locator().contents(),
                                    )
                                    .unwrap_or_else(|| dict.value(i - 1).range())
                                    .end(),
                                    parenthesized_range(
                                        dict.value(i).into(),
                                        dict.into(),
                                        checker.comment_ranges(),
                                        checker.locator().contents(),
                                    )
                                    .unwrap_or_else(|| dict.value(i).range())
                                    .end(),
                                )));
                            }
                            checker.report_diagnostic(diagnostic);
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
                            let comparable_value: ComparableExpr = dict.value(i).into();
                            if !seen_values.insert(comparable_value) {
                                diagnostic.set_fix(Fix::unsafe_edit(Edit::deletion(
                                    parenthesized_range(
                                        dict.value(i - 1).into(),
                                        dict.into(),
                                        checker.comment_ranges(),
                                        checker.locator().contents(),
                                    )
                                    .unwrap_or_else(|| dict.value(i - 1).range())
                                    .end(),
                                    parenthesized_range(
                                        dict.value(i).into(),
                                        dict.into(),
                                        checker.comment_ranges(),
                                        checker.locator().contents(),
                                    )
                                    .unwrap_or_else(|| dict.value(i).range())
                                    .end(),
                                )));
                            }
                            checker.report_diagnostic(diagnostic);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
