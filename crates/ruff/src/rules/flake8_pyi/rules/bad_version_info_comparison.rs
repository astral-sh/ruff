use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::Range;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Cmpop, Expr};

define_violation!(
    /// ## What it does
    ///
    /// Ensures that you only `<` and `>=` for version info comparisons with
    /// `sys.version_info` in `.pyi` files. All other comparisons such as
    /// `>`, `<=` and `==` are banned.
    ///
    /// ## Why is this bad?
    ///
    /// `sys.version_info > (3, 8)` will also match `3.8.10`. Similarly,
    /// `sys.version_info <= (3, 8)` will not match `3.8.10`.
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
    ///
    /// ```python
    /// import sys
    ///
    /// if sys.version_info > (3, 8):
    //     ...
    /// ```
    ///
    /// Use instead:
    ///
    /// ```python
    /// import sys
    ///
    /// if sys.version_info >= (3, 9):
    //     ...
    /// ```
    pub struct BadVersionInfoComparison;
);
impl Violation for BadVersionInfoComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use only `<` and `>=` for version info comparisons.")
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

    if !checker.resolve_call_path(left).map_or(false, |call_path| {
        call_path.as_slice() == ["sys", "version_info"]
    }) {
        return;
    }

    if !matches!(op, Cmpop::Lt | Cmpop::GtE) {
        let diagnostic = Diagnostic::new(BadVersionInfoComparison, Range::from_located(expr));
        checker.diagnostics.push(diagnostic);
    }
}
