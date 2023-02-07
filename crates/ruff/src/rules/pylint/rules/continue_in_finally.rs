use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::ast::helpers::ContinueStatementVisitor;
use crate::violation::Violation;

use ruff_macros::derive_message_formats;
use rustpython_ast::Stmt;

define_violation!(
    pub struct ContinueInFinally;
);
impl Violation for ContinueInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continue is not supported inside a finally block")
    }
}

/// PLE0116
pub fn continue_in_finally(checker: &mut Checker, finalbody: &[Stmt]) {
    if finalbody.is_empty() {
        return;
    }
    for stmt in finalbody {
        let mut visitor = ContinueStatementVisitor::default();
        let continues = visitor.visit_stmt(stmt);
        if !continues.is_empty() {
            let range = Range::from_located(stmt);
            let diagnostic = Diagnostic::new(ContinueInFinally, range);
            checker.diagnostics.push(diagnostic);
            return;
        }
    }
}
