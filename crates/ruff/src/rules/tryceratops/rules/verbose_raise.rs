use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct VerboseRaise;
);
impl Violation for VerboseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise` without specifying exception name")
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<Option<&'a Expr>>,
}

impl<'a, 'b> Visitor<'b> for RaiseStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::Raise { exc, .. } => self.raises.push(exc.as_ref().map(|expr| &**expr)),
            StmtKind::Try {
                body, finalbody, ..
            } => {
                for stmt in body.iter().chain(finalbody.iter()) {
                    visitor::walk_stmt(self, stmt);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// TRY201
pub fn verbose_raise(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        // If the handler assigned a name to the exception...
        if let ExcepthandlerKind::ExceptHandler {
            name: Some(exception_name),
            body,
            ..
        } = &handler.node
        {
            let raises = {
                let mut visitor = RaiseStatementVisitor::default();
                for stmt in body {
                    visitor.visit_stmt(stmt);
                }
                visitor.raises
            };
            for expr in raises.into_iter().flatten() {
                // ...and the raised object is bound to the same name...
                if let ExprKind::Name { id, .. } = &expr.node {
                    if id == exception_name {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(VerboseRaise, Range::from_located(expr)));
                    }
                }
            }
        }
    }
}
