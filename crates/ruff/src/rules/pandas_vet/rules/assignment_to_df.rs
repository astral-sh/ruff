use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct PandasDfVariableName;

impl Violation for PandasDfVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`df` is a bad variable name. Be kinder to your future self.")
    }
}

/// PD901
pub fn assignment_to_df(targets: &[Expr]) -> Option<Diagnostic> {
    if targets.len() != 1 {
        return None;
    }
    let target = &targets[0];
    let ExprKind::Name { id, .. } = &target.node else {
        return None;
    };
    if id != "df" {
        return None;
    }
    Some(Diagnostic::new(PandasDfVariableName, Range::from(target)))
}
