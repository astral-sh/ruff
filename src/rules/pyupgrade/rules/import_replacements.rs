use rustpython_ast::{Located, Stmt, StmtKind, AliasData};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

/// UP035
pub fn import_replacements(checker: &mut Checker, stmt: &Stmt, names: &Vec<Located<AliasData>>, module: &str) {
    // Pyupgrade only works with import_from statements, so this library does that as well
    
}
