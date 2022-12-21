use rustpython_ast::Expr;

use crate::ast::types::{Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLE0118
pub fn used_prior_global_declaration(checker: &mut Checker, name: &str, expr: &Expr) {
    let globals = match &checker.current_scope().kind {
        ScopeKind::Class(class_def) => &class_def.globals,
        ScopeKind::Function(function_def) => &function_def.globals,
        _ => return,
    };
    if let Some(stmt) = globals.get(name) {
        if checker.range_for(expr).location < stmt.location {
            checker.add_check(Check::new(
                CheckKind::UsedPriorGlobalDeclaration(name.to_string(), stmt.location.row()),
                Range::from_located(expr),
            ));
        }
    }
}
