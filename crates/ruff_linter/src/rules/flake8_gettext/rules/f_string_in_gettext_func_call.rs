use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for f-strings in `gettext` function calls.
///
/// ## Why is this bad?
/// In the `gettext` API, the `gettext` function (often aliased to `_`) returns
/// a translation of its input argument by looking it up in a translation
/// catalog.
///
/// Calling `gettext` with an f-string as its argument can cause unexpected
/// behavior. Since the f-string is resolved before the function call, the
/// translation catalog will look up the formatted string, rather than the
/// f-string template.
///
/// Instead, format the value returned by the function call, rather than
/// its argument.
///
/// ## Example
/// ```python
/// from gettext import gettext as _
///
/// name = "Maria"
/// _(f"Hello, {name}!")  # Looks for "Hello, Maria!".
/// ```
///
/// Use instead:
/// ```python
/// from gettext import gettext as _
///
/// name = "Maria"
/// _("Hello, %s!") % name  # Looks for "Hello, %s!".
/// ```
///
/// ## References
/// - [Python documentation: gettext](https://docs.python.org/3/library/gettext.html)
#[violation]
pub struct FStringInGetTextFuncCall;

impl Violation for FStringInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// INT001
pub(crate) fn f_string_in_gettext_func_call(checker: &mut Checker, args: &[Expr]) {
    if let Some(first) = args.first() {
        if first.is_f_string_expr() {
            checker
                .diagnostics
                .push(Diagnostic::new(FStringInGetTextFuncCall {}, first.range()));
        }
    }
}
