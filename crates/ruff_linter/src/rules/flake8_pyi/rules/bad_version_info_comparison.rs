use ruff_python_ast::{self as ast, CmpOp, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of comparators other than `<` and `>=` for
/// `sys.version_info` checks in `.pyi` files. All other comparators, such
/// as `>`, `<=`, and `==`, are banned.
///
/// ## Why is this bad?
/// Comparing `sys.version_info` with `==` or `<=` has unexpected behavior
/// and can lead to bugs.
///
/// For example, `sys.version_info > (3, 8)` will also match `3.8.10`,
/// while `sys.version_info <= (3, 8)` will _not_ match `3.8.10`:
///
/// ```python
/// >>> import sys
/// >>> print(sys.version_info)
/// sys.version_info(major=3, minor=8, micro=10, releaselevel='final', serial=0)
/// >>> print(sys.version_info > (3, 8))
/// True
/// >>> print(sys.version_info == (3, 8))
/// False
/// >>> print(sys.version_info <= (3, 8))
/// False
/// >>> print(sys.version_info in (3, 8))
/// False
/// ```
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info > (3, 8):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info >= (3, 9):
///     ...
/// ```
#[violation]
pub struct BadVersionInfoComparison;

impl Violation for BadVersionInfoComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `<` or `>=` for `sys.version_info` comparisons")
    }
}

/// PYI006
pub(crate) fn bad_version_info_comparison(checker: &mut Checker, test: &Expr) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return;
    };

    let ([op], [_right]) = (ops.as_slice(), comparators.as_slice()) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_call_path(left)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["sys", "version_info"]))
    {
        return;
    }

    if matches!(op, CmpOp::Lt | CmpOp::GtE) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(BadVersionInfoComparison, test.range()));
}
