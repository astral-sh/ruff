use ruff_python_ast::helpers::RaiseStatementVisitor;
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use rustpython_parser::ast::{self, Excepthandler, ExcepthandlerKind, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;
use crate::rules::tryceratops::helpers::LoggerCandidateVisitor;

/// ## What it does
/// Checks for uses of `raise` directly after a `rescue`
///
/// ## Why is this bad?
/// Catching an error just to reraise it is pointless. Instead, remove error-handling and let the error propogate naturally
///
/// ## Example
/// ```python
///
/// def foo():
///     try:
///         bar()
///     except NotImplementedError:
//          raise
/// ```
///
/// Use instead:
/// ```python
///
/// def foo():
///     bar()
/// ```
#[violation]
pub struct PointlessRaise;

impl Violation for PointlessRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove")
    }
}

// #[derive(Default)]
// struct RaiseStatementVisitor<'a> {
//     raises: Vec<&'a Stmt>,
// }

// impl<'a, 'b> StatementVisitor<'b> for RaiseStatementVisitor<'a>
// where
//     'b: 'a,
// {
//     fn visit_stmt(&mut self, stmt: &'b Stmt) {
//         match stmt.node {
//             StmtKind::Raise(_) => self.raises.push(stmt),
//             StmtKind::Try(_) | StmtKind::TryStar(_) => (),
//             _ => walk_stmt(self, stmt),
//         }
//     }
// }

/// TRY302
pub(crate) fn pointless_raise(checker: &mut Checker, body: &[Stmt], handlers: &[Excepthandler]) {
    if handlers.is_empty() {
        return;
    }

    if let Some(stmt) = body.first() {
        if let StmtKind::Raise(ast::StmtRaise { exc: None, .. }) = &stmt.node {
            checker
                .diagnostics
                .push(Diagnostic::new(PointlessRaise, stmt.range()));
        }
    }
}
