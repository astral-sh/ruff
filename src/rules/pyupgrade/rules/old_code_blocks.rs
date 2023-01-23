use rustpython_parser::ast::{Expr, Stmt, ExprKind};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// UP037
pub fn old_code_blocks(checker: &Checker, test: &Expr, body: &[Stmt], orelse: &[Stmt]) {
    // NOTE: Pyupgrade ONLY works if `sys.version_info` is on the left
    println!("\n===================\n");
    println!("{:?}", test.node);
    if let ExprKind::Compare{ left, ops, .. } = &test.node {
        if checker.resolve_call_path(left).map_or(false, |call_path| {
            call_path.as_slice() == ["six", "PY2"]
        }) {
            println!("WE HAVE SIX");
        } else {
            println!("NO SIX");
        }
        println!("{:?}\n", test.node);
        println!("{:?}\n", body);
        println!("{:?}\n", orelse);
    }
}
