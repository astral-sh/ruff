use crate::ast::helpers::ContinueStatementVisitor;
use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};

use rustpython_parser::ast::Stmt;

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
        visitor.visit_stmt(stmt);
        let continues = visitor.returns;
        if !continues.is_empty() {
            let the_continue = continues.first().unwrap();
            let range = Range::from_located(the_continue);
            let diagnostic = Diagnostic::new(ContinueInFinally, range);
            checker.diagnostics.push(diagnostic);
            return;
        }
    }
}
