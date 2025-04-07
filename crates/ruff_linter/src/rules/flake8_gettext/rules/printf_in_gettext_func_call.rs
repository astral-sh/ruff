use ruff_python_ast::{self as ast, Expr, Operator};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for printf-style formatted strings in `gettext` function calls.
///
/// ## Why is this bad?
/// In the `gettext` API, the `gettext` function (often aliased to `_`) returns
/// a translation of its input argument by looking it up in a translation
/// catalog.
///
/// Calling `gettext` with a formatted string as its argument can cause
/// unexpected behavior. Since the formatted string is resolved before the
/// function call, the translation catalog will look up the formatted string,
/// rather than the printf-style template.
///
/// Instead, format the value returned by the function call, rather than
/// its argument.
///
/// ## Example
/// ```python
/// from gettext import gettext as _
///
/// name = "Maria"
/// _("Hello, %s!" % name)  # Looks for "Hello, Maria!".
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
/// - [Python documentation: `gettext` — Multilingual internationalization services](https://docs.python.org/3/library/gettext.html)
#[derive(ViolationMetadata)]
pub(crate) struct PrintfInGetTextFuncCall;

impl Violation for PrintfInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "printf-style format is resolved before function call; consider `_(\"string %s\") % arg`"
            .to_string()
    }
}

/// INT003
pub(crate) fn printf_in_gettext_func_call(checker: &Checker, args: &[Expr]) {
    if let Some(first) = args.first() {
        if let Expr::BinOp(ast::ExprBinOp {
            op: Operator::Mod,
            left,
            ..
        }) = &first
        {
            if left.is_string_literal_expr() {
                checker
                    .report_diagnostic(Diagnostic::new(PrintfInGetTextFuncCall {}, first.range()));
            }
        }
    }
}
