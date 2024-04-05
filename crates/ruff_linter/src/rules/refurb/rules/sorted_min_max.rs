use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_diagnostics::FixAvailability;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_ast::ExprCall;
use ruff_python_ast::ExprNumberLiteral;
use ruff_python_ast::ExprSubscript;
use ruff_python_ast::ExprUnaryOp;
use ruff_python_ast::UnaryOp;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use ruff_python_ast::Number;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `sorted()` to get the min and max values of a list.
///
/// ## Why is this bad?
/// Using `sorted()` to get the min and max values of a list is inefficient.
///
/// ## Example
/// ```python
/// nums = [3, 1, 4, 1, 5]
/// lowest = sorted(nums)[0]
/// highest = sorted(nums)[-1]
/// highest = sorted(nums, reverse=True)[0]
/// ```
///
/// Use instead:
/// ```python
/// nums = [3, 1, 4, 1, 5]
/// lowest = min(nums)
/// highest = max(nums)
/// ```
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)

#[violation]
pub struct SortedMinMax {
    min_max: MinMax,
    expression: SourceCodeSnippet,
    replacement: SourceCodeSnippet,
}

impl Violation for SortedMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `min` and `max` over `sorted()` to get the min and max values of a list")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `min` and `max`"))
    }
}

/// FURB192
pub(crate) fn sorted_min_max(checker: &mut Checker, subscript: &ExprSubscript) {
    if subscript.ctx.is_store() || subscript.ctx.is_del() {
        return;
    }

    let ExprSubscript { slice, value, .. } = &subscript;

    // Early return if index is not unary or a number literal
    if !(slice.is_number_literal_expr() || slice.is_unary_op_expr()) {
        return;
    }

    let Expr::Call(ExprCall { func, .. }) = value.as_ref() else {
        return;
    };
    // Check if the value is a call to `sorted()`
    if !matches!(func.as_ref(), Expr::Name(name) if name.id == "sorted" && checker.semantic().is_builtin(name.id.as_str()))
    {
        return;
    };

    let Some(index) = (match slice.as_ref() {
        // [0]
        Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(index),
            ..
        }) if *index == 0 => index.as_i64(),

        // [-1]
        Expr::UnaryOp(ExprUnaryOp {
            op: UnaryOp::USub,
            operand,
            ..
        }) => {
            let Expr::NumberLiteral(ExprNumberLiteral {
                value: Number::Int(index),
                ..
            }) = operand.as_ref()
            else {
                return;
            };
            if *index == 1 {
                index.as_i64()
            } else {
                None
            }
        }
        _ => return,
    }) else {
        return;
    };

    let replacement = format!("{}(sorted())", if index == 0 { "min" } else { "max" });

    let mut diagnostic = Diagnostic::new(
        SortedMinMax {
            min_max: if index == 0 { MinMax::Min } else { MinMax::Max },
            expression: SourceCodeSnippet::from_str(checker.locator().slice(subscript)),
            replacement: SourceCodeSnippet::new(replacement),
        },
        subscript.range(),
    );
    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
        index.to_string(),
        subscript.start(),
        subscript.end(),
    )));
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MinMax {
    Min,
    Max,
}
// TODO:
// - Caveat reverse=True with -1 as unsafe
