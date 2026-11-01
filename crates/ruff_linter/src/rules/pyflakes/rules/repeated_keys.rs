use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::{ComparableExpr, HashableExpr};
use ruff_python_ast::{self as ast, Expr, ExprKind};
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
pub(crate) fn repeated_keys(checker: &Checker, dict: ast::ExprDict<'_>) {
    let items = dict.items();
    if items.len() < 2 {
        return;
    }

    let mut seen_keys = FxHashMap::with_capacity_and_hasher(items.len(), FxBuildHasher);
    let mut seen_values = FxHashMap::default();

    for (i, item) in items.iter().enumerate() {
        let Some(key) = item.key() else {
            continue;
        };

        let (first_index, first_key) = match seen_keys.entry(HashableExpr::from(key)) {
            Entry::Vacant(entry) => {
                entry.insert((i, key));
                continue;
            }
            Entry::Occupied(entry) => *entry.get(),
        };

        let mut diagnostic = match key.classify() {
            key if is_literal_key(key.view())
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
            ExprKind::Name(_) if checker.is_rule_enabled(Rule::MultiValueRepeatedKeyVariable) => {
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
            FxHashSet::from_iter([ComparableExpr::from(items.at(first_index).value())])
        });
        if !values.insert(ComparableExpr::from(item.value())) {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::deletion(
                items.at(i - 1).end(),
                item.end(),
            )));
        }
    }
}

fn is_literal_key(expr: Expr<'_>) -> bool {
    match expr.fields() {
        ast::ExprFields::StringLiteral(_)
        | ast::ExprFields::BytesLiteral(_)
        | ast::ExprFields::NumberLiteral(_)
        | ast::ExprFields::BooleanLiteral(_)
        | ast::ExprFields::NoneLiteral(_)
        | ast::ExprFields::EllipsisLiteral(_)
        | ast::ExprFields::Tuple(_)
        | ast::ExprFields::FString(_) => true,
        ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields {
            op: ast::UnaryOp::UAdd | ast::UnaryOp::USub,
            operand,
            ..
        }) => signed_number_literal(operand).is_some(),
        ast::ExprFields::BinOp(ast::ExprBinOpFields {
            left,
            op: ast::Operator::Add | ast::Operator::Sub,
            right,
            ..
        }) => {
            signed_number_literal(left).is_some_and(|number| {
                matches!(number.value(), ast::Number::Int(_) | ast::Number::Float(_))
            }) && matches!(
                right.fields(),
                ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields {
                    value: ast::Number::Complex { .. },
                    ..
                })
            )
        }
        _ => false,
    }
}

fn signed_number_literal(expr: Expr<'_>) -> Option<ast::ExprNumberLiteral<'_>> {
    match expr.fields() {
        ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields { node: number, .. }) => {
            Some(number)
        }
        ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields {
            op: ast::UnaryOp::UAdd | ast::UnaryOp::USub,
            operand,
            ..
        }) => signed_number_literal(operand),
        _ => None,
    }
}
