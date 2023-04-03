use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

#[violation]
pub struct LoadBeforeGlobalDeclaration {
    pub name: String,
    pub line: usize,
}

impl Violation for LoadBeforeGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoadBeforeGlobalDeclaration { name, line } = self;
        format!("Name `{name}` is used prior to global declaration on line {line}")
    }
}
/// PLE0118
pub fn load_before_global_declaration(checker: &mut Checker, name: &str, expr: &Expr) {
    let globals = match &checker.ctx.scope().kind {
        ScopeKind::Class(class_def) => &class_def.globals,
        ScopeKind::Function(function_def) => &function_def.globals,
        _ => return,
    };
    if let Some(stmt) = globals.get(name) {
        if expr.location < stmt.location {
            checker.diagnostics.push(Diagnostic::new(
                LoadBeforeGlobalDeclaration {
                    name: name.to_string(),
                    line: stmt.location.row(),
                },
                Range::from(expr),
            ));
        }
    }
}
