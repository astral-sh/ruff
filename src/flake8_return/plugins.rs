use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::check_ast::Checker;
use crate::checks::{Branch, CheckCode, CheckKind};
use crate::flake8_return::helpers::result_exists;
use crate::flake8_return::visitor::{ReturnVisitor, Stack};
use crate::Check;

/// RET501
fn unnecessary_return_none(checker: &mut Checker, stack: &Stack) {
    for (stmt, expr) in &stack.returns {
        if let Some(expr) = expr {
            if matches!(
                expr.node,
                ExprKind::Constant {
                    value: Constant::None,
                    ..
                }
            ) {
                checker.add_check(Check::new(
                    CheckKind::UnnecessaryReturnNone,
                    Range::from_located(stmt),
                ));
            }
        }
    }
}

/// RET502
fn implicit_return_value(checker: &mut Checker, stack: &Stack) {
    for (stmt, expr) in &stack.returns {
        if expr.is_none() {
            checker.add_check(Check::new(
                CheckKind::ImplicitReturnValue,
                Range::from_located(stmt),
            ));
        }
    }
}

/// RET503
fn implicit_return(checker: &mut Checker, last_stmt: &Stmt) {
    match &last_stmt.node {
        StmtKind::If { body, orelse, .. } => {
            if body.is_empty() || orelse.is_empty() {
                checker.add_check(Check::new(
                    CheckKind::ImplicitReturn,
                    Range::from_located(last_stmt),
                ));
                return;
            }

            if let Some(last_stmt) = body.last() {
                implicit_return(checker, last_stmt);
            }
            if let Some(last_stmt) = orelse.last() {
                implicit_return(checker, last_stmt);
            }
        }
        StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
            if let Some(last_stmt) = orelse.last() {
                implicit_return(checker, last_stmt);
            } else if let Some(last_stmt) = body.last() {
                implicit_return(checker, last_stmt);
            }
        }
        StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => {
            if let Some(last_stmt) = body.last() {
                implicit_return(checker, last_stmt);
            }
        }
        StmtKind::Assert { test, .. }
            if matches!(
                test.node,
                ExprKind::Constant {
                    value: Constant::Bool(false),
                    ..
                }
            ) => {}
        StmtKind::Return { .. }
        | StmtKind::While { .. }
        | StmtKind::Raise { .. }
        | StmtKind::Try { .. } => {}
        _ => {
            checker.add_check(Check::new(
                CheckKind::ImplicitReturn,
                Range::from_located(last_stmt),
            ));
        }
    }
}

fn has_refs_before_next_assign(id: &str, return_location: Location, stack: &Stack) -> bool {
    let mut before_assign: &Location = &Location::default();
    let mut after_assign: Option<&Location> = None;
    if let Some(assigns) = stack.assigns.get(&id) {
        for location in assigns.iter().sorted() {
            if location.row() > return_location.row() {
                after_assign = Some(location);
                break;
            }
            if location.row() <= return_location.row() {
                before_assign = location;
            }
        }
    }

    if let Some(refs) = stack.refs.get(&id) {
        for location in refs {
            if location.row() == return_location.row() {
                continue;
            }
            if let Some(after_assign) = after_assign {
                if before_assign.row() < location.row() && location.row() <= after_assign.row() {
                    return true;
                }
            } else if before_assign.row() < location.row() {
                return true;
            }
        }
    }
    false
}

fn has_refs_or_assigns_within_try_or_loop(id: &str, stack: &Stack) -> bool {
    if let Some(refs) = stack.refs.get(&id) {
        for location in refs {
            for (try_location, try_end_location) in &stack.tries {
                if try_location.row() < location.row() && location.row() <= try_end_location.row() {
                    return true;
                }
            }
            for (loop_location, loop_end_location) in &stack.loops {
                if loop_location.row() < location.row() && location.row() <= loop_end_location.row()
                {
                    return true;
                }
            }
        }
    }
    if let Some(refs) = stack.assigns.get(&id) {
        for location in refs {
            for (try_location, try_end_location) in &stack.tries {
                if try_location.row() < location.row() && location.row() <= try_end_location.row() {
                    return true;
                }
            }
            for (loop_location, loop_end_location) in &stack.loops {
                if loop_location.row() < location.row() && location.row() <= loop_end_location.row()
                {
                    return true;
                }
            }
        }
    }
    false
}

/// RET504
fn unnecessary_assign(checker: &mut Checker, stack: &Stack, expr: &Expr) {
    if let ExprKind::Name { id, .. } = &expr.node {
        if !stack.assigns.contains_key(id.as_str()) {
            return;
        }

        if !stack.refs.contains_key(id.as_str()) {
            checker.add_check(Check::new(
                CheckKind::UnnecessaryAssign,
                Range::from_located(expr),
            ));
            return;
        }

        if has_refs_before_next_assign(id, expr.location, stack)
            || has_refs_or_assigns_within_try_or_loop(id, stack)
        {
            return;
        }

        checker.add_check(Check::new(
            CheckKind::UnnecessaryAssign,
            Range::from_located(expr),
        ));
    }
}

/// RET505, RET506, RET507, RET508
fn superfluous_else_node(checker: &mut Checker, stmt: &Stmt, branch: Branch) -> bool {
    if let StmtKind::If { body, .. } = &stmt.node {
        for child in body {
            if matches!(child.node, StmtKind::Return { .. }) {
                if checker.settings.enabled.contains(&CheckCode::RET505) {
                    checker.add_check(Check::new(
                        CheckKind::SuperfluousElseReturn(branch),
                        Range::from_located(stmt),
                    ));
                }
                return true;
            }
            if matches!(child.node, StmtKind::Break) {
                if checker.settings.enabled.contains(&CheckCode::RET508) {
                    checker.add_check(Check::new(
                        CheckKind::SuperfluousElseBreak(branch),
                        Range::from_located(stmt),
                    ));
                }
                return true;
            }
            if matches!(child.node, StmtKind::Raise { .. }) {
                if checker.settings.enabled.contains(&CheckCode::RET506) {
                    checker.add_check(Check::new(
                        CheckKind::SuperfluousElseRaise(branch),
                        Range::from_located(stmt),
                    ));
                }
                return true;
            }
            if matches!(child.node, StmtKind::Continue) {
                if checker.settings.enabled.contains(&CheckCode::RET507) {
                    checker.add_check(Check::new(
                        CheckKind::SuperfluousElseContinue(branch),
                        Range::from_located(stmt),
                    ));
                }
                return true;
            }
        }
    }
    false
}

/// RET505, RET506, RET507, RET508
fn superfluous_elif(checker: &mut Checker, stack: &Stack) -> bool {
    for stmt in &stack.elifs {
        if superfluous_else_node(checker, stmt, Branch::Elif) {
            return true;
        }
    }
    false
}

/// RET505, RET506, RET507, RET508
fn superfluous_else(checker: &mut Checker, stack: &Stack) -> bool {
    for stmt in &stack.ifs {
        if let StmtKind::If { orelse, .. } = &stmt.node {
            if !orelse.is_empty() {
                if superfluous_else_node(checker, stmt, Branch::Else) {
                    return true;
                }
            }
        }
    }
    false
}

/// Run all checks from the `flake8-return` plugin.
pub fn function(checker: &mut Checker, body: &[Stmt]) {
    // Skip empty functions.
    if body.is_empty() {
        return;
    }

    // Find the last statement in the function.
    let last_stmt = body.last().unwrap();

    // Skip functions that consist of a single return statement.
    if body.len() == 1 && matches!(last_stmt.node, StmtKind::Return { .. }) {
        return;
    }

    // Traverse the function body, to collect the stack.
    let stack = {
        let mut visitor = ReturnVisitor::default();
        for stmt in body {
            visitor.visit_stmt(stmt);
        }
        visitor.stack
    };

    if checker.settings.enabled.contains(&CheckCode::RET505)
        || checker.settings.enabled.contains(&CheckCode::RET506)
        || checker.settings.enabled.contains(&CheckCode::RET507)
        || checker.settings.enabled.contains(&CheckCode::RET508)
    {
        if superfluous_elif(checker, &stack) {
            return;
        }
        if superfluous_else(checker, &stack) {
            return;
        }
    }

    // Skip any functions without return statements.
    if stack.returns.is_empty() {
        return;
    }

    if !result_exists(&stack.returns) {
        if checker.settings.enabled.contains(&CheckCode::RET501) {
            unnecessary_return_none(checker, &stack);
        }
        return;
    }

    if checker.settings.enabled.contains(&CheckCode::RET502) {
        implicit_return_value(checker, &stack);
    }
    if checker.settings.enabled.contains(&CheckCode::RET503) {
        implicit_return(checker, last_stmt);
    }

    if checker.settings.enabled.contains(&CheckCode::RET504) {
        for (_, expr) in &stack.returns {
            if let Some(expr) = expr {
                unnecessary_assign(checker, &stack, expr);
            }
        }
    }
}
