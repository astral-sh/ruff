use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprCall, Identifier};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `int` with an explicit base in which a string expression
/// is stripped of its leading prefix (i.e., `0b`, `0o`, or `0x`).
///
/// ## Why is this bad?
/// Given an integer string with a prefix (e.g., `0xABC`), Python can automatically
/// determine the base of the integer by the prefix without needing to specify
/// it explicitly.
///
/// Instead of `int(num[2:], 16)`, use `int(num, 0)`, which will automatically
/// deduce the base based on the prefix.
///
/// ## Example
/// ```python
/// num = "0xABC"
///
/// if num.startswith("0b"):
///     i = int(num[2:], 2)
/// elif num.startswith("0o"):
///     i = int(num[2:], 8)
/// elif num.startswith("0x"):
///     i = int(num[2:], 16)
///
/// print(i)
/// ```
///
/// Use instead:
/// ```python
/// num = "0xABC"
///
/// i = int(num, 0)
///
/// print(i)
/// ```
///
/// ## Fix safety
/// The rule's fix is marked as unsafe, as Ruff cannot guarantee that the
/// argument to `int` will remain valid when its base is included in the
/// function call.
///
/// ## References
/// - [Python documentation: `int`](https://docs.python.org/3/library/functions.html#int)
#[violation]
pub struct IntOnSlicedStr {
    base: u8,
}

impl AlwaysFixableViolation for IntOnSlicedStr {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IntOnSlicedStr { base } = self;
        format!("Use of `int` with explicit `base={base}` after removing prefix")
    }

    fn fix_title(&self) -> String {
        format!("Replace with `base=0`")
    }
}

pub(crate) fn int_on_sliced_str(checker: &mut Checker, call: &ExprCall) {
    // Verify that the function is `int`.
    if !checker.semantic().match_builtin_expr(&call.func, "int") {
        return;
    }

    // There must be exactly two arguments (e.g., `int(num[2:], 16)`).
    let (expression, base) = match (
        call.arguments.args.as_ref(),
        call.arguments.keywords.as_ref(),
    ) {
        ([expression], [base]) if base.arg.as_ref().map(Identifier::as_str) == Some("base") => {
            (expression, &base.value)
        }
        ([expression, base], []) => (expression, base),
        _ => {
            return;
        }
    };

    // The base must be a number literal with a value of 2, 8, or 16.
    let Some(base_u8) = base
        .as_number_literal_expr()
        .and_then(|base| base.value.as_int())
        .and_then(ruff_python_ast::Int::as_u8)
    else {
        return;
    };
    if !matches!(base_u8, 2 | 8 | 16) {
        return;
    }

    // Determine whether the expression is a slice of a string (e.g., `num[2:]`).
    let Expr::Subscript(expr_subscript) = expression else {
        return;
    };
    let Expr::Slice(expr_slice) = expr_subscript.slice.as_ref() else {
        return;
    };
    if expr_slice.upper.is_some() || expr_slice.step.is_some() {
        return;
    }
    if !expr_slice
        .lower
        .as_ref()
        .and_then(|expr| expr.as_number_literal_expr())
        .and_then(|expr| expr.value.as_int())
        .is_some_and(|expr| expr.as_u8() == Some(2))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(IntOnSlicedStr { base: base_u8 }, call.range());
    diagnostic.set_fix(Fix::unsafe_edits(
        Edit::range_replacement(
            checker.locator().slice(&*expr_subscript.value).to_string(),
            expression.range(),
        ),
        [Edit::range_replacement("0".to_string(), base.range())],
    ));
    checker.diagnostics.push(diagnostic);
}
