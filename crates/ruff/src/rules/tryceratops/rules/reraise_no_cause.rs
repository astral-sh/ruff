use rustpython_parser::ast::{ExprKind, Stmt};

use ruff_macros::{derive_message_formats, violation};

use crate::ast::helpers::RaiseStatementVisitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

#[violation]
pub struct ReraiseNoCause;

impl Violation for ReraiseNoCause {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise from` to specify exception cause")
    }
}

/// TRY200
pub fn reraise_no_cause(checker: &mut Checker, body: &[Stmt]) {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        for stmt in body {
            visitor.visit_stmt(stmt);
        }
        visitor.raises
    };

    for (range, exc, cause) in raises {
        if exc.map_or(false, |expr| matches!(expr.node, ExprKind::Call { .. })) && cause.is_none() {
            checker
                .diagnostics
                .push(Diagnostic::new(ReraiseNoCause, range));
        }
    }
}
