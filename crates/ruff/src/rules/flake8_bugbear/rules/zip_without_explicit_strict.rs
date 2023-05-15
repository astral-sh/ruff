use rustpython_parser::ast::{self, Expr, ExprKind, Keyword};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct ZipWithoutExplicitStrict;

impl Violation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`zip()` without an explicit `strict=` parameter")
    }
}

/// B905
pub(crate) fn zip_without_explicit_strict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    kwargs: &[Keyword],
) {
    if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
        if id == "zip"
            && checker.ctx.is_builtin("zip")
            && !kwargs.iter().any(|keyword| {
                keyword
                    .node
                    .arg
                    .as_ref()
                    .map_or(false, |name| name == "strict")
            })
        {
            checker
                .diagnostics
                .push(Diagnostic::new(ZipWithoutExplicitStrict, expr.range()));
        }
    }
}
