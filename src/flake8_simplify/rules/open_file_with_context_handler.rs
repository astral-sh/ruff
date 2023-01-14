use rustpython_ast::Expr;
use rustpython_parser::ast::StmtKind;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// SIM115
pub fn open_file_with_context_handler(checker: &mut Checker, func: &Expr) {
    if checker
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path == ["", "open"])
    {
        if checker.is_builtin("open") {
            match checker.current_stmt().node {
                StmtKind::With { .. } => (),
                _ => {
                    checker.diagnostics.push(Diagnostic::new(
                        violations::OpenFileWithContextHandler,
                        Range::from_located(func),
                    ));
                }
            }
        }
    }
}
