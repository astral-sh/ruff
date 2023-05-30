use rustpython_parser::ast::{Expr, Ranged};
use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of PEP 585- and PEP 604-style type annotations in Python
/// modules that lack the required `from __future__ import annotations` import
/// for compatibility with older Python versions.
///
/// ## Why is this bad?
/// Using PEP 585 and PEP 604 style annotations without a `from __future__ import
/// annotations` import will cause runtime errors on Python versions prior to
/// 3.9 and 3.10, respectively.
///
/// By adding the `__future__` import, the interpreter will no longer interpret
/// annotations at evaluation time, making the code compatible with both past
/// and future Python versions.
///
/// ## Example
/// ```python
/// def func(obj: dict[str, int | None]) -> None:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
///
/// def func(obj: dict[str, int | None]) -> None:
///     ...
/// ```
#[violation]
pub struct FutureRequiredTypeAnnotation {
    reason: Reason,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The type annotation is written in PEP 585 style (e.g., `list[int]`).
    PEP585,
    /// The type annotation is written in PEP 604 style (e.g., `int | None`).
    PEP604,
}

impl fmt::Display for Reason {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Reason::PEP585 => fmt.write_str("PEP 585 collection"),
            Reason::PEP604 => fmt.write_str("PEP 604 union"),
        }
    }
}

impl Violation for FutureRequiredTypeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureRequiredTypeAnnotation { reason } = self;
        format!("Missing `from __future__ import annotations`, but uses {reason}")
    }
}

/// FA102
pub(crate) fn future_required_type_annotation(checker: &mut Checker, expr: &Expr, reason: Reason) {
    checker.diagnostics.push(Diagnostic::new(
        FutureRequiredTypeAnnotation { reason },
        expr.range(),
    ));
}
