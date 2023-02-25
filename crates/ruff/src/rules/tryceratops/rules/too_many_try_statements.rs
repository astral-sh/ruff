use crate::ast::types::Range;
use crate::ast::visitor::{self, Visitor};
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Stmt, StmtKind};

define_violation!(
    /// ### What it does
    /// Ensures your function is not frequently interrupted by try statements
    ///
    /// ### Why is this bad?
    /// Frequently interupations make your code harder to read
    ///
    /// ### Example
    /// ```python
    /// def main_function():
    ///     try:
    ///         receipt_note = receipt_service.create(order_id)
    ///     except Exception:
    ///         logger.exception("log")
    ///         raise
    ///
    ///     try:
    ///         broker.emit_receipt_note(receipt_note)
    ///     except Exception:
    ///         logger.exception("log")
    ///         raise
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def main_function():
    ///     try:
    ///         receipt_note = receipt_service.create(order_id)
    ///         broker.emit_receipt_note(receipt_note)
    ///     except ReceiptNoteCreationFailed:
    ///         logger.exception("log")
    ///         raise
    ///     except NoteEmissionFailed:
    ///         logger.exception("log")
    ///         raise
    /// ```
    pub struct TooManyTryStatements;
);

impl Violation for TooManyTryStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Convert your code into one large try statement")
    }
}

#[derive(Default)]
struct TryStatementVisitor<'a> {
    raises: Vec<&'a Stmt>,
}

impl<'a, 'b> Visitor<'b> for TryStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt.node {
            // REVIEWER: Should we also check for StmtKind::TryStar???
            StmtKind::Try { .. } => {
                self.raises.push(stmt);
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// TRY101
pub fn too_many_try_statements(checker: &mut Checker, body: &[Stmt]) {
    let mut total_tries = 0;
    for bod in body {
        let mut visitor = TryStatementVisitor::default();
        visitor.visit_stmt(bod);
        total_tries += visitor.raises.len();
    }
    if total_tries > 1 {
        let Some(beginning) = body.first() else {return};
        let Some(end) = body.last() else {return};
        let range = Range::new(beginning.location, end.end_location.unwrap());
        checker
            .diagnostics
            .push(Diagnostic::new(TooManyTryStatements, range));
    }
}
