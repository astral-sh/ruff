use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::OneIndexed;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

#[violation]
pub struct LoadBeforeGlobalDeclaration {
    name: String,
    line: OneIndexed,
}

impl Violation for LoadBeforeGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoadBeforeGlobalDeclaration { name, line } = self;
        format!("Name `{name}` is used prior to global declaration on line {line}")
    }
}
/// PLE0118
pub(crate) fn load_before_global_declaration(checker: &mut Checker, name: &str, expr: &Expr) {
    let globals = match &checker.ctx.scope().kind {
        ScopeKind::Class(class_def) => &class_def.globals,
        ScopeKind::Function(function_def) => &function_def.globals,
        _ => return,
    };
    if let Some(stmt) = globals.get(name) {
        if expr.start() < stmt.start() {
            #[allow(deprecated)]
            let location = checker.locator.compute_source_location(stmt.start());
            checker.diagnostics.push(Diagnostic::new(
                LoadBeforeGlobalDeclaration {
                    name: name.to_string(),
                    line: location.row,
                },
                expr.range(),
            ));
        }
    }
}
