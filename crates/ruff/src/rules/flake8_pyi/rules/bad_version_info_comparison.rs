use rustpython_parser::ast::{Cmpop, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usages of comparators other than `<` and `>=` for
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
        format!("Use `<` or `>=` for version info comparisons")
    }
}

/// PYI006
pub fn bad_version_info_comparison(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    let ([op], [_right]) = (ops, comparators) else {
        return;
    };

    if !checker
        .ctx
        .resolve_call_path(left)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["sys", "version_info"]
        })
    {
        return;
    }

    if !matches!(op, Cmpop::Lt | Cmpop::GtE) {
        let diagnostic = Diagnostic::new(BadVersionInfoComparison, Range::from(expr));
        checker.diagnostics.push(diagnostic);
    }
}
