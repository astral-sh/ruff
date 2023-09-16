use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `six.PY3`.
///
/// ## Why is this bad?
/// `six.PY3` will evaluate to `False` on Python 4 and greater. This is likely
/// unintended, and may cause code intended to run on Python 2 to run on Python 4
/// too.
///
/// Instead, use `not six.PY2` to validate that the current Python major version is
/// _not_ equal to 2, to future-proof the code.
///
/// ## Example
/// ```python
/// import six
///
/// six.PY3  # `False` on Python 4.
/// ```
///
/// Use instead:
/// ```python
/// import six
///
/// not six.PY2  # `True` on Python 4.
/// ```
///
/// ## References
/// - [PyPI: `six`](https://pypi.org/project/six/)
/// - [Six documentation: `six.PY2`](https://six.readthedocs.io/#six.PY2)
/// - [Six documentation: `six.PY3`](https://six.readthedocs.io/#six.PY3)
#[violation]
pub struct SixPY3;

impl Violation for SixPY3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`six.PY3` referenced (python4), use `not six.PY2`")
    }
}

/// YTT202
pub(crate) fn name_or_attribute(checker: &mut Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(expr)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["six", "PY3"]))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(SixPY3, expr.range()));
    }
}
