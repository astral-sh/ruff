use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;

#[violation]
pub struct VerboseRaise;

impl Violation for VerboseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise` without specifying exception name")
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<(Option<&'a Expr>, Option<&'a Expr>)>,
}

impl<'a, 'b> Visitor<'b> for RaiseStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::Raise { exc, cause } => self.raises.push((exc.as_deref(), cause.as_deref())),
            StmtKind::Try {
                body, finalbody, ..
            }
            | StmtKind::TryStar {
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
            for (exc, cause) in raises {
                if cause.is_some() {
                    continue;
                }
                if let Some(exc) = exc {
                    // ...and the raised object is bound to the same name...
                    if let ExprKind::Name { id, .. } = &exc.node {
                        if id == exception_name {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(VerboseRaise, Range::from(exc)));
                        }
                    }
                }
            }
        }
    }
}
