use rustpython_ast::{Excepthandler, ExcepthandlerKind, Stmt, StmtKind};

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// SIM105
pub fn use_contextlib_suppress(
    xxxxxxxx: &mut xxxxxxxx,
    stmt: &Stmt,
    handlers: &[Excepthandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) {
    if handlers.len() != 1 || !orelse.is_empty() || !finalbody.is_empty() {
        return;
    }
    let handler = &handlers[0];
    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
    if body.len() == 1 {
        if matches!(body[0].node, StmtKind::Pass) {
            let handler_names: Vec<_> = helpers::extract_handler_names(handlers)
                .into_iter()
                .map(|v| v.join("."))
                .collect();
            let exception = if handler_names.is_empty() {
                "Exception".to_string()
            } else {
                handler_names.join(", ")
            };
            let check = Diagnostic::new(
                violations::UseContextlibSuppress(exception),
                Range::from_located(stmt),
            );
            xxxxxxxx.diagnostics.push(check);
        }
    }
}
