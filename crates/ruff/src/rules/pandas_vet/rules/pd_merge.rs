use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct PandasUseOfPdMerge;

impl Violation for PandasUseOfPdMerge {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use `.merge` method instead of `pd.merge` function. They have equivalent \
             functionality."
        )
    }
}

/// PD015
pub fn use_of_pd_merge(func: &Expr) -> Option<Diagnostic> {
    if let ExprKind::Attribute { attr, value, .. } = &func.node {
        if let ExprKind::Name { id, .. } = &value.node {
            if id == "pd" && attr == "merge" {
                return Some(Diagnostic::new(PandasUseOfPdMerge, Range::from(func)));
            }
        }
    }
    None
}
