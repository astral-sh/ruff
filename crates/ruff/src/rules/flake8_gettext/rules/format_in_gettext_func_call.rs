use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `str.format` calls in `gettext` function calls.
///
/// ## Why is this bad?
/// In the `gettext` API, the `gettext` function (usually aliased to `_`)
/// returns a translation of the given string by looking it up in a translation
/// catalogue.
///
/// Formatting strings in the function call means the formatted string will be
/// passed to the function, which will then look it up in the translation
/// catalogue.
///
/// This is likely unintended. Even if such behavior is intended, it is
/// error-prone as the translation catalogue may not contain the formatted
/// string.
///
/// Instead, consider formatting the result of the function call.
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
/// - [Python documentation: gettext](https://docs.python.org/3/library/gettext.html)
#[violation]
pub struct FormatInGetTextFuncCall;

impl Violation for FormatInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`format` method argument is resolved before function call; consider `_(\"string %s\") % arg`")
    }
}

/// INT002
pub(crate) fn format_in_gettext_func_call(checker: &mut Checker, args: &[Expr]) {
    if let Some(first) = args.first() {
        if let Expr::Call(ast::ExprCall { func, .. }) = &first {
            if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() {
                if attr == "format" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(FormatInGetTextFuncCall {}, first.range()));
                }
            }
        }
    }
}
