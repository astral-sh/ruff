use rustpython_parser::ast::{self, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct BreakOutsideLoop;

impl Violation for BreakOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`break` outside loop")
    }
}

/// F701
pub fn break_outside_loop<'a>(
    stmt: &'a Stmt,
    parents: &mut impl Iterator<Item = &'a Stmt>,
) -> Option<Diagnostic> {
    let mut allowed: bool = false;
    let mut child = stmt;
    for parent in parents {
        match &parent.node {
            StmtKind::For(ast::StmtFor { orelse, .. })
            | StmtKind::AsyncFor(ast::StmtAsyncFor { orelse, .. })
            | StmtKind::While(ast::StmtWhile { orelse, .. }) => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }
            StmtKind::FunctionDef(_) | StmtKind::AsyncFunctionDef(_) | StmtKind::ClassDef(_) => {
                break;
            }
            _ => {}
        }
        child = parent;
    }

    if allowed {
        None
    } else {
        Some(Diagnostic::new(BreakOutsideLoop, stmt.range()))
    }
}
