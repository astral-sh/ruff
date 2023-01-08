use rustpython_ast::Expr;

use crate::ast::types::{Range, ScopeKind};
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLE0118
pub fn used_prior_global_declaration(xxxxxxxx: &mut xxxxxxxx, name: &str, expr: &Expr) {
    let globals = match &xxxxxxxx.current_scope().kind {
        ScopeKind::Class(class_def) => &class_def.globals,
        ScopeKind::Function(function_def) => &function_def.globals,
        _ => return,
    };
    if let Some(stmt) = globals.get(name) {
        if expr.location < stmt.location {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::UsedPriorGlobalDeclaration(name.to_string(), stmt.location.row()),
                Range::from_located(expr),
            ));
        }
    }
}
