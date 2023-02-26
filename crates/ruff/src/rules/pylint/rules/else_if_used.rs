use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Stmt, StmtKind};

use crate::ast::helpers::identifier_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct ElseIfUsed {}
);

impl Violation for ElseIfUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using \"elif\" instead of \"else\" then \"if\" to remove one indentation level")
    }
}

/// PLR5501
pub fn else_if_used(
    stmt: &Stmt,
    body: &[Stmt],
    _orelse: &[Stmt],
    locator: &Locator,
) -> Option<Diagnostic> {
    if let StmtKind::If { orelse, .. } = &stmt.node {
        // If the body contains more than just the orelse, can't apply.
        if body.len() == 1 {
            if let [first, ..] = &orelse[..] {
                if matches!(first.node, StmtKind::If { .. }) {
                    // check the source if this is else then if or elif
                    // we do this to see if they are on the same line
                    if stmt.location.row() != first.location.row() {
                        return Some(Diagnostic::new(
                            ElseIfUsed {},
                            identifier_range(stmt, locator),
                        ));
                    }
                }
            }
        }
    }
    None
}
