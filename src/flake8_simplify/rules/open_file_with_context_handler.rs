use rustpython_ast::Expr;
use rustpython_parser::ast::StmtKind;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// SIM115
pub fn open_file_with_context_handler(checker: &mut Checker, func: &Expr) {
    if match_call_path(
        &dealias_call_path(collect_call_paths(func), &checker.import_aliases),
        "",
        "open",
        &checker.from_imports,
    ) {
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
