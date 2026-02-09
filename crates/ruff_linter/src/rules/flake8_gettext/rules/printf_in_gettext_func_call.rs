use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::flake8_gettext::is_ngettext_call;

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
/// ## Options
///
/// - `lint.flake8-gettext.function-names`
///
/// ## References
/// - [Python documentation: `gettext` — Multilingual internationalization services](https://docs.python.org/3/library/gettext.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.260")]
pub(crate) struct PrintfInGetTextFuncCall {
    is_plural: bool,
}

impl Violation for PrintfInGetTextFuncCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_plural {
            "printf-style format in plural argument is resolved before function call".to_string()
        } else {
            "printf-style format is resolved before function call; consider `_(\"string %s\") % arg`"
                .to_string()
        }
    }
}

/// INT003
pub(crate) fn printf_in_gettext_func_call(checker: &Checker, func: &Expr, args: &[Expr]) {
    // Check first argument (singular)
    if let Some(first) = args.first() {
        if is_printf_format(first) {
            checker.report_diagnostic(PrintfInGetTextFuncCall { is_plural: false }, first.range());
        }
    }

    // Check second argument (plural) for ngettext calls
    if is_ngettext_call(checker, func)
        && let Some(second) = args.get(1)
        && is_printf_format(second)
    {
        checker.report_diagnostic(PrintfInGetTextFuncCall { is_plural: true }, second.range());
    }
}

/// Return `true` if `expr` is a printf-style formatting expression, such as
/// `"%s" % 123`.
fn is_printf_format(expr: &Expr) -> bool {
    let Expr::BinOp(ast::ExprBinOp {
        op: Operator::Mod,
        left,
        ..
    }) = expr
    else {
        return false;
    };

    left.is_string_literal_expr()
}
