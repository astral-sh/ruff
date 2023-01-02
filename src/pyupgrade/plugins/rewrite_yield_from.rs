use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

fn update_content(checker: &mut Checker, stmt: &Stmt, iter: &Expr) {
    let mut check = Check::new(CheckKind::RewriteYieldFrom, Range::from_located(stmt));
    let contents = checker
        .locator
        .slice_source_code_range(&Range::from_located(iter));
    let final_contents = format!("yield from {}", contents);
    // FOR REVIEWER: The stmt does not include comments made after the last
    // code in the for loop, which causes our version to still be "correct",
    // but to different from pyupgrade. See tests that causes difference here:
    // https://github.com/asottile/pyupgrade/blob/main/tests/features/yield_from_test.py#L52-L68
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            final_contents,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}

fn get_items(expr: &Expr) -> Vec<String> {
    match &expr.node {
        ExprKind::Name { id, .. } => vec![id.to_string()],
        ExprKind::Tuple { elts, .. } => elts.iter().map(|e| get_items(e)).flatten().collect(),
        _ => vec![],
    }
}

#[derive(Debug)]
struct YieldFrom {
    statement: Stmt,
    iter: Expr,
    yield_items: Vec<String>,
    target_items: Vec<String>,
}

impl YieldFrom {
    fn new(statement: &Stmt, iter: &Expr, expression: &Expr, target: &Expr) -> Option<Self> {
        if let ExprKind::Yield { value } = &expression.node {
            let mut base = Self {
                statement: statement.to_owned(),
                iter: iter.to_owned(),
                yield_items: vec![],
                target_items: vec![],
            };
            if let Some(clean_value) = value {
                base.yield_items = get_items(clean_value);
            };
            base.target_items = get_items(target);
            return Some(base);
        }
        None
    }

    /// Checks if the items and order of yield is the same as in for
    fn check_items(&self) -> bool {
        self.yield_items == self.target_items
    }
}

fn get_yields_from(stmt: &Stmt, yields: &mut Vec<YieldFrom>) {
    match &stmt.node {
        StmtKind::For {
            target,
            body,
            orelse,
            iter,
            ..
        } => {
            // If there is an else statement we should not refactor
            if !orelse.is_empty() {
                return;
            }
            // Don't run if there is logic besides the yield
            if body.len() > 1 {
                return;
            }
            let first_statement = match body.get(0) {
                None => return,
                Some(item) => item,
            };
            if let StmtKind::Expr { value } = &first_statement.node {
                if let ExprKind::Yield { .. } = &value.node {
                    let the_item = YieldFrom::new(stmt, iter, value, target).unwrap();
                    yields.push(the_item);
                }
            }
        }
        StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } => {
            for item in body {
                get_yields_from(item, yields);
            }
        }
        _ => (),
    }
}

pub fn rewrite_yield_from(checker: &mut Checker, stmt: &Stmt) {
    let mut yields: Vec<YieldFrom> = vec![];
    get_yields_from(stmt, &mut yields);
    for item in yields {
        if item.check_items() {
            update_content(checker, &item.statement, &item.iter);
        }
    }
}
