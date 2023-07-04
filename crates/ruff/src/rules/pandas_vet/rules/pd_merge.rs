use rustpython_parser::ast::{self, Expr, Ranged};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
pub(crate) fn use_of_pd_merge(checker: &mut Checker, func: &Expr) {
    if let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func {
        if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
            if id == "pd" && attr == "merge" {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PandasUseOfPdMerge, func.range()));
            }
        }
    }
}
