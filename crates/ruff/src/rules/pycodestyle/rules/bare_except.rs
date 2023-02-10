use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Excepthandler, Expr, Stmt, StmtKind};

use crate::ast::helpers::except_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct BareExcept;
);
impl Violation for BareExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use bare `except`")
    }
}

/// E722
pub fn bare_except(
    type_: Option<&Expr>,
    body: &[Stmt],
    handler: &Excepthandler,
    locator: &Locator,
) -> Option<Diagnostic> {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt.node, StmtKind::Raise { exc: None, .. }))
    {
        Some(Diagnostic::new(BareExcept, except_range(handler, locator)))
    } else {
        None
    }
}
