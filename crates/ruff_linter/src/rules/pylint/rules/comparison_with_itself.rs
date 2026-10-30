use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCompare;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for operations that compare a name to itself.
///
/// ## Why is this bad?
/// Comparing a name to itself typically results in a truthy value, and is
/// likely a mistake.
///
/// ## Example
/// ```python
/// foo == foo
/// ```
///
/// In some cases, self-comparisons are used to determine whether a float is
/// NaN. Instead, prefer `math.isnan`:
/// ```python
/// import math
///
/// math.isnan(foo)
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.273", category = Category::Suspicious)]
pub(crate) struct ComparisonWithItself {
    actual: SourceCodeSnippet,
}

impl Violation for ComparisonWithItself {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(actual) = self.actual.full_display() {
            format!("Name compared with itself, consider replacing `{actual}`")
        } else {
            "Name compared with itself".to_string()
        }
    }
}

/// PLR0124
pub(crate) fn comparison_with_itself(checker: &Checker, compare: flat::View<'_, ExprCompare>) {
    for (left, op, right) in compare.iter() {
        match (left.classify(), right.classify()) {
            // Ex) `foo == foo`
            (flat::ExprKind::Name(left_name), flat::ExprKind::Name(right_name))
                if left_name.id() == right_name.id() =>
            {
                let actual = format!(
                    "{} {} {}",
                    checker.locator().slice(left),
                    op,
                    checker.locator().slice(right)
                );
                checker.report_diagnostic(
                    ComparisonWithItself {
                        actual: SourceCodeSnippet::new(actual),
                    },
                    left_name.range(),
                );
            }
            // Ex) `id(foo) == id(foo)`
            (flat::ExprKind::Call(left_call), flat::ExprKind::Call(right_call)) => {
                // Both calls must take a single argument, of the same name.
                if !left_call.arguments().keywords().is_empty()
                    || !right_call.arguments().keywords().is_empty()
                {
                    continue;
                }
                let Some([flat::ExprKind::Name(left_arg)]) = left_call
                    .arguments()
                    .args()
                    .as_array()
                    .map(|items| items.map(flat::View::<ruff_python_ast::Expr>::classify))
                else {
                    continue;
                };
                let Some([flat::ExprKind::Name(right_right)]) = right_call
                    .arguments()
                    .args()
                    .as_array()
                    .map(|items| items.map(flat::View::<ruff_python_ast::Expr>::classify))
                else {
                    continue;
                };
                if left_arg.id() != right_right.id() {
                    continue;
                }

                // Both calls must be to the same function.
                let semantic = checker.semantic();
                let Some(left_name) = semantic.resolve_builtin_symbol(left_call.func()) else {
                    continue;
                };
                let Some(right_name) = semantic.resolve_builtin_symbol(right_call.func()) else {
                    continue;
                };
                if left_name != right_name {
                    continue;
                }

                // The call must be to pure function, like `id`.
                if matches!(
                    left_name,
                    "id" | "len" | "type" | "int" | "bool" | "str" | "repr" | "bytes"
                ) {
                    let actual = format!(
                        "{} {} {}",
                        checker.locator().slice(left),
                        op,
                        checker.locator().slice(right)
                    );
                    checker.report_diagnostic(
                        ComparisonWithItself {
                            actual: SourceCodeSnippet::new(actual),
                        },
                        left_call.range(),
                    );
                }
            }
            _ => {}
        }
    }
}
