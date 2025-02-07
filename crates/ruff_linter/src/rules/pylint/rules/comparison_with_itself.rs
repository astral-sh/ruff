use itertools::Itertools;

use crate::fix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for operations that compare a name to itself.
///
/// ## Why is this bad?
/// Comparing a name to itself always results in the same value, and is likely
/// a mistake.
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
pub(crate) fn comparison_with_itself(
    checker: &Checker,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    for ((left, right), op) in std::iter::once(left)
        .chain(comparators)
        .tuple_windows()
        .zip(ops)
    {
        match (left, right) {
            // Ex) `foo == foo`
            (Expr::Name(left_name), Expr::Name(right_name)) if left_name.id == right_name.id => {
                let actual = format!(
                    "{} {} {}",
                    checker.locator().slice(left),
                    op,
                    checker.locator().slice(right)
                );
                checker.report_diagnostic(Diagnostic::new(
                    ComparisonWithItself {
                        actual: SourceCodeSnippet::new(actual),
                    },
                    left_name.range(),
                ));
            }
            // Ex) `id(foo) == id(foo)`
            (Expr::Call(left_call), Expr::Call(right_call)) => {
                // Both calls must take a single argument, of the same name.
                if !left_call.arguments.keywords.is_empty()
                    || !right_call.arguments.keywords.is_empty()
                {
                    continue;
                }
                let [Expr::Name(left_arg)] = &*left_call.arguments.args else {
                    continue;
                };
                let [Expr::Name(right_right)] = &*right_call.arguments.args else {
                    continue;
                };
                if left_arg.id != right_right.id {
                    continue;
                }

                // Both calls must be to the same function.
                let semantic = checker.semantic();
                let Some(left_name) = semantic.resolve_builtin_symbol(&left_call.func) else {
                    continue;
                };
                let Some(right_name) = semantic.resolve_builtin_symbol(&right_call.func) else {
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
                    checker.report_diagnostic(Diagnostic::new(
                        ComparisonWithItself {
                            actual: SourceCodeSnippet::new(actual),
                        },
                        left_call.range(),
                    ));
                }
            }
            _ => {}
        }
    }
}
