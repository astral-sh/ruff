use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_diagnostics::FixAvailability;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_ast::ExprSubscript;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
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
pub struct SortedMinMax;

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

    let Some(name) = match_sorted_min_max(subscript, checker.semantic()) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(SortedMinMax, subscript.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
        name.to_string(),
        subscript.start(),
        subscript.end(),
    )));
    checker.diagnostics.push(diagnostic);
}

pub(crate) fn match_sorted_min_max(
    ExprSubscript { value, slice, .. }: &ExprSubscript,
    semantic: &SemanticModel,
) -> Option<i8> {
    // Early return if index is not 0 or 1
    let index = {
        if let Expr::NumberLiteral(number_literal) = slice.as_ref() {
            if let Number::Int(number) = &number_literal.value {
                return number
                    .as_i8()
                    .and_then(|n| if n == 0 || n == 1 { Some(n) } else { None });
            }
        }
        None
    };

    // Check if the value is a call to `sorted()`
    if let Expr::Call(call) = value.as_ref() {
        if let Expr::Name(name) = call.func.as_ref() {
            if name.id == "sorted" && semantic.is_builtin(name.id.as_str()) {
                return index;
            }
        }
    }

    None
}

// TODO:
// - Caveat reverse=True with -1 as unsafe
