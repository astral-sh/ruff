use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for missing `from __future__ import annotations` imports upon
/// detecting type annotations that are written in PEP 585 or PEP 604 style but
/// the target Python version doesn't support them without the import.
///
/// ## Why is this bad?
///
/// Using PEP 585 and PEP 604 style annotations without the import will cause
/// runtime errors on Python versions older than 3.9 and 3.10, respectively.
///
/// By adding the `__future__` import, the interpreter will no longer interpret
/// annotations at evaluation time, making the code compatible with both older
/// and newer Python versions.
///
/// ## Example
///
/// ```python
/// def function(a_dict: dict[str, int | None]) -> None:
///     a_list: list[str] = []
///     a_list.append("hello")
/// ```
/// would raise an exception at runtime before Python 3.9 and Python 3.10.
#[violation]
pub struct MissingFutureAnnotationsImportNewStyle {
    kind: String,
    expr: String,
}

impl Violation for MissingFutureAnnotationsImportNewStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingFutureAnnotationsImportNewStyle { kind, expr } = self;
        format!("Missing `from __future__ import annotations`, but uses {kind} `{expr}`")
    }
}

/// FA102
pub(crate) fn missing_future_annotations_new_style(checker: &mut Checker, kind: &str, expr: &Expr) {
    checker.diagnostics.push(Diagnostic::new(
        MissingFutureAnnotationsImportNewStyle {
            kind: kind.to_string(),
            expr: checker.locator.slice(expr.range()).to_string(),
        },
        expr.range(),
    ));
}
