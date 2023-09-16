use itertools::Itertools;

use crate::autofix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::CmpOpExt;

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
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[violation]
pub struct ComparisonWithItself {
    actual: SourceCodeSnippet,
}

impl Violation for ComparisonWithItself {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ComparisonWithItself { actual } = self;
        if let Some(actual) = actual.full_display() {
            format!("Name compared with itself, consider replacing `{actual}`")
        } else {
            format!("Name compared with itself")
        }
    }
}

/// PLR0124
pub(crate) fn comparison_with_itself(
    checker: &mut Checker,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    for ((left, right), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows()
        .zip(ops)
    {
        match (left, right) {
            // Ex) `foo == foo`
            (Expr::Name(left_name), Expr::Name(right_name)) if left_name.id == right_name.id => {
                let actual = format!(
                    "{} {} {}",
                    checker.locator().slice(left),
                    CmpOpExt::from(op),
                    checker.locator().slice(right)
                );
                checker.diagnostics.push(Diagnostic::new(
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
                let [Expr::Name(left_arg)] = left_call.arguments.args.as_slice() else {
                    continue;
                };
                let [Expr::Name(right_right)] = right_call.arguments.args.as_slice() else {
                    continue;
                };
                if left_arg.id != right_right.id {
                    continue;
                }

                // Both calls must be to the same function.
                let Expr::Name(left_func) = left_call.func.as_ref() else {
                    continue;
                };
                let Expr::Name(right_func) = right_call.func.as_ref() else {
                    continue;
                };
                if left_func.id != right_func.id {
                    continue;
                }

                // The call must be to pure function, like `id`.
                if matches!(
                    left_func.id.as_str(),
                    "id" | "len" | "type" | "int" | "bool" | "str" | "repr" | "bytes"
                ) && checker.semantic().is_builtin(&left_func.id)
                {
                    let actual = format!(
                        "{} {} {}",
                        checker.locator().slice(left),
                        CmpOpExt::from(op),
                        checker.locator().slice(right)
                    );
                    checker.diagnostics.push(Diagnostic::new(
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
