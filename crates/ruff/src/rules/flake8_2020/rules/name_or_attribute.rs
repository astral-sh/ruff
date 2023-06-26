use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `six.PY3`.
///
/// ## Why is this bad?
/// `six.PY3` will evaluate to `False` on Python 4 and greater. This is likely
/// unintended, and may cause code intended to be run on Python 2 to be
/// executed on Python 4.
///
/// Instead, use `not six.PY2` to check if the Python version is not 2. This is
/// more future-proof.
///
/// ## Example
/// ```python
/// import six
///
/// six.PY3  # If using Python 4, this evaluates to `False`.
/// ```
///
/// Use instead:
/// ```python
/// import six
///
/// not six.PY2  # If using Python 4, this evaluates to `True`.
/// ```
///
/// ## References
/// - [PyPI: `six`](https://pypi.org/project/six/)
/// - [Six documentation: `six.PY3`](https://six.readthedocs.io/#six.PY3)
/// - [Six documentation: `six.PY2`](https://six.readthedocs.io/#six.PY2)
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
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["six", "PY3"])
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(SixPY3, expr.range()));
    }
}
