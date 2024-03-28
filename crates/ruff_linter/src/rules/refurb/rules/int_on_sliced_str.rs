use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprCall, Identifier};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of converting a string starting with `0b`, `0o`, or `0x` to an int, by removing
/// two first symbols and explicitly set the base.
///
/// ## Why is this bad?
/// Rather than set the base explicitly, call the `int` with the base of zero, and let automatically
/// deduce the base by the prefix.
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
/// The rule's fix is marked as unsafe, because there is no way for `ruff` to detect whether
/// the stripped prefix was a valid Python `int` prefix.
/// ## References
/// - [Python documentation: `int`](https://docs.python.org/3/library/functions.html#int)
#[violation]
pub struct IntOnSlicedStr;

impl AlwaysFixableViolation for IntOnSlicedStr {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `int` with the explicit `base` after removing the prefix")
    }

    fn fix_title(&self) -> String {
        format!("Use `int` with `base` 0 instead")
    }
}

pub(crate) fn int_on_sliced_str(checker: &mut Checker, call: &ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|name| matches!(name.segments(), ["" | "builtins", "int"]))
    {
        return;
    }
    let (arg, base_keyword, base) = match (
        call.arguments.args.as_ref(),
        call.arguments.keywords.as_ref(),
    ) {
        ([arg], [base_kw_arg])
            if base_kw_arg.arg.as_ref().map(Identifier::as_str) == Some("base") =>
        {
            (arg, "base=", &base_kw_arg.value)
        }
        ([arg, base_arg], []) => (arg, "", base_arg),
        _ => {
            return;
        }
    };
    if !base
        .as_number_literal_expr()
        .and_then(|base| base.value.as_int())
        .is_some_and(|base| matches!(base.as_u8(), Some(2 | 8 | 16)))
    {
        return;
    };

    let Expr::Subscript(expr_subscript) = arg else {
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
        .and_then(|x| x.as_number_literal_expr())
        .and_then(|x| x.value.as_int())
        .is_some_and(|x| x.as_u8() == Some(2))
    {
        return;
    }
    checker
        .diagnostics
        .push(
            Diagnostic::new(IntOnSlicedStr, call.range).with_fix(Fix::unsafe_edit(
                Edit::range_replacement(
                    format!(
                        "int({}, {base_keyword}0)",
                        checker.locator().slice(expr_subscript.value.as_ref())
                    ),
                    call.range,
                ),
            )),
        );
}
