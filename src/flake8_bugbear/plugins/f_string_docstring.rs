use rustpython_ast::{ExprKind, Stmt, StmtKind};

use crate::ast::helpers;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B021
pub fn f_string_docstring(xxxxxxxx: &mut xxxxxxxx, body: &[Stmt]) {
    let Some(stmt) = body.first() else {
        return;
    };
    let StmtKind::Expr { value } = &stmt.node else {
        return;
    };
    let ExprKind::JoinedStr { .. } = value.node else {
        return;
    };
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::FStringDocstring,
        helpers::identifier_range(stmt, xxxxxxxx.locator),
    ));
}
