use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::{ComparableExpr, HashableExpr};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;
use crate::registry::Rule;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
///
/// This rule's fix is marked as unsafe because removing a repeated dictionary key
/// may delete comments that are attached to the removed key-value pair. This can also change
/// the program's behavior if the value expressions have side effects.
///
/// ## References
/// - [Python documentation: Dictionaries](https://docs.python.org/3/tutorial/datastructures.html#dictionaries)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.30", category = Category::Correctness)]
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
/// ## Fix safety
///
/// This rule's fix is marked as unsafe because removing a repeated dictionary key
/// may delete comments that are attached to the removed key-value pair. This can also change
/// the program's behavior if the value expressions have side effects.
///
/// ## References
/// - [Python documentation: Dictionaries](https://docs.python.org/3/tutorial/datastructures.html#dictionaries)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.30", category = Category::Correctness)]
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
    if dict.len() < 2 {
        return;
    }

    let mut seen_keys = FxHashMap::with_capacity_and_hasher(dict.len(), FxBuildHasher);
    let mut seen_values = FxHashMap::default();

    for (i, ast::DictItem { key, value }) in dict.iter().enumerate() {
        let Some(key) = key else {
            continue;
        };

        let (first_index, first_key) = match seen_keys.entry(HashableExpr::from(key)) {
            Entry::Vacant(entry) => {
                entry.insert((i, key));
                continue;
            }
            Entry::Occupied(entry) => *entry.get(),
        };

        let mut diagnostic = match key {
            key if is_literal_key(key)
                && checker.is_rule_enabled(Rule::MultiValueRepeatedKeyLiteral) =>
            {
                checker.report_diagnostic(
                    MultiValueRepeatedKeyLiteral {
                        name: SourceCodeSnippet::from_str(checker.locator().slice(key)),
                        existing: SourceCodeSnippet::from_str(checker.locator().slice(first_key)),
                    },
                    key.range(),
                )
            }
            Expr::Name(_) if checker.is_rule_enabled(Rule::MultiValueRepeatedKeyVariable) => {
                checker.report_diagnostic(
                    MultiValueRepeatedKeyVariable {
                        name: SourceCodeSnippet::from_str(checker.locator().slice(key)),
                    },
                    key.range(),
                )
            }
            _ => continue,
        };

        // Comparing values is only necessary to decide whether a repeated key can be fixed.
        let values = seen_values.entry(first_index).or_insert_with(|| {
            FxHashSet::from_iter([ComparableExpr::from(dict.value(first_index))])
        });
        if !values.insert(ComparableExpr::from(value)) {
            let previous = dict.value(i - 1);
            diagnostic.set_fix(Fix::unsafe_edit(Edit::deletion(
                parenthesized_range(previous.into(), dict.into(), checker.tokens())
                    .unwrap_or_else(|| previous.range())
                    .end(),
                parenthesized_range(value.into(), dict.into(), checker.tokens())
                    .unwrap_or_else(|| value.range())
                    .end(),
            )));
        }
    }
}

fn is_literal_key(expr: &Expr) -> bool {
    match expr {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Tuple(_)
        | Expr::FString(_) => true,
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::UAdd | ast::UnaryOp::USub,
            operand,
            ..
        }) => signed_number_literal(operand).is_some(),
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: ast::Operator::Add | ast::Operator::Sub,
            right,
            ..
        }) => {
            signed_number_literal(left).is_some_and(|number| {
                matches!(number.value, ast::Number::Int(_) | ast::Number::Float(_))
            }) && matches!(
                right.as_ref(),
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Complex { .. },
                    ..
                })
            )
        }
        _ => false,
    }
}

fn signed_number_literal(expr: &Expr) -> Option<&ast::ExprNumberLiteral> {
    match expr {
        Expr::NumberLiteral(number) => Some(number),
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::UAdd | ast::UnaryOp::USub,
            operand,
            ..
        }) => signed_number_literal(operand),
        _ => None,
    }
}
