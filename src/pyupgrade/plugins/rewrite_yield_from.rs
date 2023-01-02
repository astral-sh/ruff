use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

fn update_content(checker: &mut Checker, for_stmt: &Stmt, iter: &Expr) {
    let mut check = Check::new(CheckKind::RewriteYieldFrom, Range::from_located(for_stmt));
    let contents = checker
        .locator
        .slice_source_code_range(&Range::from_located(iter));
    let final_contents = format!("yield from {contents}");
    // FOR REVIEWER: The stmt does not include comments made after the last
    // code in the for loop, which causes our version to still be "correct",
    // but to different from pyupgrade. See tests that causes difference here:
    // https://github.com/asottile/pyupgrade/blob/main/tests/features/yield_from_test.py#L52-L68
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            final_contents,
            for_stmt.location,
            for_stmt.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}

fn get_items(expr: &Expr) -> Vec<String> {
    match &expr.node {
        ExprKind::Name { id, .. } => vec![id.to_string()],
        ExprKind::Tuple { elts, .. } => elts.iter().flat_map(get_items).collect(),
        _ => vec![],
    }
}

#[derive(Debug)]
struct YieldFrom {
    // The entire for statement
    statement: Stmt,
    // The iterator in the for statement
    iter: Expr,
    // The yield part of the statement, this is needed to know when to start
    // looking for names
    yield_: Expr,
    yield_items: Vec<String>,
    target_items: Vec<String>,
}

impl YieldFrom {
    fn new(statement: &Stmt, iter: &Expr, expression: &Expr, target: &Expr) -> Option<Self> {
        if let ExprKind::Yield { value } = &expression.node {
            let mut base = Self {
                statement: statement.clone(),
                iter: iter.clone(),
                yield_: expression.clone(),
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

    /// If any of the items in the for loop and yield are used elsewhere in the
    /// function, we cannot rewrite the yield. This is a rough way of checking
    /// for the variables later on. Returns true if they are NOT used later,
    /// which means the for yield should be rewritten.
    fn check_target_items(&self, checker: &Checker, function_stmt: &Stmt) -> bool {
        // Get everything before the for loop but still inside the original function
        let before_range = Range::new(function_stmt.location, self.statement.location);
        // Get the everything inside the original function, but after the given yield
        // statement
        let after_range = Range::new(
            self.yield_.end_location.unwrap(),
            function_stmt.end_location.unwrap(),
        );
        let before_contents = checker.locator.slice_source_code_range(&before_range);
        let after_contents = checker.locator.slice_source_code_range(&after_range);
        // Check each item in the for loop targets to see if they are used before (and
        // not used to assign)
        for item in &self.target_items {
            let mut next_must_be_assign = false;
            for (_, tok, _) in lexer::make_tokenizer(&before_contents).flatten() {
                // If we find a matching name we need to check if it is being assigned
                if let Tok::Name { name } = tok {
                    if &name == item {
                        next_must_be_assign = true;
                    }
                // The name found in the last token is not beign assigned, dont
                // fix this one
                } else if Tok::Equal != tok && next_must_be_assign {
                    return false;
                // Just in case the last one was an equal sign, we need to set
                // the variable back to false to avoid false
                // positives in the future
                } else {
                    next_must_be_assign = false;
                }
            }
        }
        // Check each item in the for loop targets to see if they are used later
        for item in &self.target_items {
            for (_, tok, _) in lexer::make_tokenizer(&after_contents).flatten() {
                if let Tok::Name { name } = tok {
                    if &name == item {
                        return false;
                    }
                }
            }
        }
        true
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

/// UP028
pub fn rewrite_yield_from(checker: &mut Checker, stmt: &Stmt) {
    let mut yields: Vec<YieldFrom> = vec![];
    get_yields_from(stmt, &mut yields);
    for item in yields {
        if item.check_items() {
            if item.check_target_items(checker, stmt) {
                update_content(checker, &item.statement, &item.iter);
            }
        }
    }
}
