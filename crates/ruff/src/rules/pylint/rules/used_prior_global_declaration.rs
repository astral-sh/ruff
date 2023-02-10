use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::{Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UsedPriorGlobalDeclaration {
        pub name: String,
        pub line: usize,
    }
);
impl Violation for UsedPriorGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UsedPriorGlobalDeclaration { name, line } = self;
        format!("Name `{name}` is used prior to global declaration on line {line}")
    }
}
/// PLE0118
pub fn used_prior_global_declaration(checker: &mut Checker, name: &str, expr: &Expr) {
    let globals = match &checker.current_scope().kind {
        ScopeKind::Class(class_def) => &class_def.globals,
        ScopeKind::Function(function_def) => &function_def.globals,
        _ => return,
    };
    if let Some(stmt) = globals.get(name) {
        if expr.location < stmt.location {
            checker.diagnostics.push(Diagnostic::new(
                UsedPriorGlobalDeclaration {
                    name: name.to_string(),
                    line: stmt.location.row(),
                },
                Range::from_located(expr),
            ));
        }
    }
}
