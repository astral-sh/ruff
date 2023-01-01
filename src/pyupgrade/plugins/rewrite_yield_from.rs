use rustpython_ast::{Expr, Stmt, ExprKind, StmtKind};

use crate::checks::{Check, CheckKind};
use crate::checkers::ast::Checker;
use crate::ast::types::Range;
use crate::autofix::Fix;

pub fn rewrite_yield_from(checker: &mut Checker, stmt: &Stmt, iter: &Expr, body: &Vec<Stmt>) {
    // We only want to do this if the yield is the first statement, otherwise
    // there could be logic that we ignore
    let first_statement = match body.get(0) {
        None => return,
        Some(item) => item,
    };
    if let StmtKind::Expr { value } = &first_statement.node {
        if let ExprKind::Yield { .. } = &value.node {
            let mut check = Check::new(CheckKind::RewriteYieldFrom, Range::from_located(stmt));
            let contents = checker.locator.slice_source_code_range(&Range::from_located(iter));
            let final_contents = format!("yield from {}", contents);
            if checker.patch(check.kind.code()) {
                check.amend(Fix::replacement(
                    final_contents,
                    stmt.location,
                    stmt.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
    println!("");
}
