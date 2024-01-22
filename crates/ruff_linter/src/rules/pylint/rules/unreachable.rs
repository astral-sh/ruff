use ast::statement_visitor::{walk_body, walk_stmt, StatementVisitor};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unreachable code. That is, code that comes after a `return` or
/// `raise` statement, since these statements will always exit the current
/// function.
///
/// ## Why is this bad?
/// Unreachable code is a sign of a mistake, and can be confusing to readers.
///
#[violation]
pub struct Unreachable {
    kind: UnreachableKind,
}

impl Violation for Unreachable {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            UnreachableKind::Return => format!("Code after return is unreachable"),
            UnreachableKind::Raise => format!("Code after raise is unreachable"),
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
enum UnreachableKind {
    Return,
    Raise,
}

#[derive(Debug, Default)]
struct UnreachableVisitor {
    final_stmt: Option<TextRange>, // the raise/return statement range end
    current_body_end: Option<TextRange>, // the body's last statement range end
    kind: Option<UnreachableKind>,
}

impl StatementVisitor<'_> for UnreachableVisitor {
    fn visit_body(&mut self, body: &[Stmt]) {
        if self.final_stmt.is_none() {
            let Some(last) = body.last() else {
                return;
            };
            self.current_body_end = Some(last.range());
            walk_body(self, body);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Raise(_) => {
                if self.final_stmt.is_none() {
                    self.final_stmt = Some(stmt.range());
                    self.kind = Some(UnreachableKind::Raise);
                }
            }
            Stmt::Return(_) => {
                if self.final_stmt.is_none() {
                    self.final_stmt = Some(stmt.range());
                    self.kind = Some(UnreachableKind::Return);
                }
            }
            Stmt::FunctionDef(_) => {
                // Don't recurse.
            }
            _ => {
                if self.final_stmt.is_none() {
                    walk_stmt(self, stmt);
                }
            }
        }
    }
}

/// PLW0101
pub(crate) fn unreachable(checker: &mut Checker, body: &[Stmt]) {
    let mut visitor = UnreachableVisitor::default();
    visitor.visit_body(body);

    let Some(kind) = visitor.kind else {
        return;
    };

    let Some(final_stmt) = visitor.final_stmt else {
        return;
    };

    let Some(body_end) = visitor.current_body_end else {
        return;
    };

    if body_end == final_stmt {
        // they're the same statement
        return;
    }

    if body_end.end() < final_stmt.end() {
        // they're not in the same branch anymore
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        Unreachable { kind },
        TextRange::new(final_stmt.start(), body_end.end()),
    ));
}
