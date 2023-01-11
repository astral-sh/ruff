use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, Stmt};
use rustpython_parser::ast::StmtKind;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::{Range, RefEquality};
use crate::registry::Diagnostic;
use crate::violations;

/// SIM115
pub fn open_file_with_context_handler(
    func: &Expr,
    parents: &[RefEquality<Stmt>],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
    let call_path = dealias_call_path(collect_call_paths(func), import_aliases);

    if match_call_path(&call_path, "", "open", from_imports) {
        if let Some(parent) = parents.iter().rev().next() {
            match parent.node {
                StmtKind::With { .. } => return None,
                _ => {
                    return Some(Diagnostic::new(
                        violations::OpenFileWithContextHandler,
                        Range::from_located(func),
                    ))
                }
            };
        }
    }
    None
}
