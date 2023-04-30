use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::{self, Visitor};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `raise` statements within `try` blocks.
///
/// ## Why is this bad?
/// Raising and catching exceptions within the same `try` block is redundant,
/// as the code can be refactored to avoid the `try` block entirely.
///
/// Alternatively, the `raise` can be moved within an inner function, making
/// the exception reusable across multiple call sites.
///
/// ## Example
/// ```python
/// def bar():
///     pass
///
///
/// def foo():
///     try:
///         a = bar()
///         if not a:
///             raise ValueError
///     except ValueError:
///         raise
/// ```
///
/// Use instead:
/// ```python
/// def bar():
///     raise ValueError
///
///
/// def foo():
///     try:
///         a = bar()  # refactored `bar` to raise `ValueError`
///     except ValueError:
///         raise
/// ```
#[violation]
pub struct RaiseWithinTry;

impl Violation for RaiseWithinTry {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Abstract `raise` to an inner function")
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<&'a Stmt>,
}

impl<'a, 'b> Visitor<'b> for RaiseStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt.node {
            StmtKind::Raise { .. } => self.raises.push(stmt),
            StmtKind::Try { .. } | StmtKind::TryStar { .. } => (),
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// TRY301
pub fn raise_within_try(checker: &mut Checker, body: &[Stmt]) {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        for stmt in body {
            visitor.visit_stmt(stmt);
        }
        visitor.raises
    };

    for stmt in raises {
        checker
            .diagnostics
            .push(Diagnostic::new(RaiseWithinTry, stmt.range()));
    }
}
