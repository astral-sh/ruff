use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::flake8_gettext::is_ngettext_call;

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
/// ## Options
///
/// - `lint.flake8-gettext.function-names`
///
/// ## References
/// - [Python documentation: `gettext` — Multilingual internationalization services](https://docs.python.org/3/library/gettext.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.260")]
pub(crate) struct FStringInGetTextFuncCall {
    is_plural: bool,
}

impl Violation for FStringInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_plural {
            "f-string in plural argument is resolved before function call".to_string()
        } else {
            "f-string is resolved before function call; consider `_(\"string %s\") % arg`"
                .to_string()
        }
    }
}

/// INT001
pub(crate) fn f_string_in_gettext_func_call(checker: &Checker, func: &Expr, args: &[Expr]) {
    // Check first argument (singular)
    if let Some(first) = args.first() {
        if first.is_f_string_expr() {
            checker.report_diagnostic(FStringInGetTextFuncCall { is_plural: false }, first.range());
        }
    }

    // Check second argument (plural) for ngettext calls
    if is_ngettext_call(checker, func)
        && let Some(second) = args.get(1)
        && second.is_f_string_expr()
    {
        checker.report_diagnostic(FStringInGetTextFuncCall { is_plural: true }, second.range());
    }
}
