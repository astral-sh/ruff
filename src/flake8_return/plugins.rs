use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::ast::whitespace::indentation;
use crate::autofix::Fix;
use crate::flake8_return::helpers::result_exists;
use crate::flake8_return::visitor::{ReturnVisitor, Stack};
use crate::registry::{Branch, RuleCode};
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// RET501
fn unnecessary_return_none(xxxxxxxx: &mut xxxxxxxx, stack: &Stack) {
    for (stmt, expr) in &stack.returns {
        let Some(expr) = expr else {
            continue;
        };
        if !matches!(
            expr.node,
            ExprKind::Constant {
                value: Constant::None,
                ..
            }
        ) {
            continue;
        }
        let mut check =
            Diagnostic::new(violations::UnnecessaryReturnNone, Range::from_located(stmt));
        if xxxxxxxx.patch(&RuleCode::RET501) {
            check.amend(Fix::replacement(
                "return".to_string(),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        }
        xxxxxxxx.diagnostics.push(check);
    }
}

/// RET502
fn implicit_return_value(xxxxxxxx: &mut xxxxxxxx, stack: &Stack) {
    for (stmt, expr) in &stack.returns {
        if expr.is_some() {
            continue;
        }
        let mut check = Diagnostic::new(violations::ImplicitReturnValue, Range::from_located(stmt));
        if xxxxxxxx.patch(&RuleCode::RET502) {
            check.amend(Fix::replacement(
                "return None".to_string(),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        }
        xxxxxxxx.diagnostics.push(check);
    }
}

/// RET503
fn implicit_return(xxxxxxxx: &mut xxxxxxxx, last_stmt: &Stmt) {
    match &last_stmt.node {
        StmtKind::If { body, orelse, .. } => {
            if body.is_empty() || orelse.is_empty() {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::ImplicitReturn,
                    Range::from_located(last_stmt),
                ));
                return;
            }

            if let Some(last_stmt) = body.last() {
                implicit_return(xxxxxxxx, last_stmt);
            }
            if let Some(last_stmt) = orelse.last() {
                implicit_return(xxxxxxxx, last_stmt);
            }
        }
        StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
            if let Some(last_stmt) = orelse.last() {
                implicit_return(xxxxxxxx, last_stmt);
            } else if let Some(last_stmt) = body.last() {
                implicit_return(xxxxxxxx, last_stmt);
            }
        }
        StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => {
            if let Some(last_stmt) = body.last() {
                implicit_return(xxxxxxxx, last_stmt);
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
            let mut check =
                Diagnostic::new(violations::ImplicitReturn, Range::from_located(last_stmt));
            if xxxxxxxx.patch(&RuleCode::RET503) {
                let mut content = String::new();
                content.push_str(&indentation(xxxxxxxx, last_stmt));
                content.push_str("return None");
                content.push('\n');
                check.amend(Fix::insertion(
                    content,
                    Location::new(last_stmt.end_location.unwrap().row() + 1, 0),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
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
fn unnecessary_assign(xxxxxxxx: &mut xxxxxxxx, stack: &Stack, expr: &Expr) {
    if let ExprKind::Name { id, .. } = &expr.node {
        if !stack.assigns.contains_key(id.as_str()) {
            return;
        }

        if !stack.refs.contains_key(id.as_str()) {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::UnnecessaryAssign,
                Range::from_located(expr),
            ));
            return;
        }

        if has_refs_before_next_assign(id, expr.location, stack)
            || has_refs_or_assigns_within_try_or_loop(id, stack)
        {
            return;
        }

        if stack.non_locals.contains(id.as_str()) {
            return;
        }

        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::UnnecessaryAssign,
            Range::from_located(expr),
        ));
    }
}

/// RET505, RET506, RET507, RET508
fn superfluous_else_node(xxxxxxxx: &mut xxxxxxxx, stmt: &Stmt, branch: Branch) -> bool {
    let StmtKind::If { body, .. } = &stmt.node else {
        return false;
    };
    for child in body {
        if matches!(child.node, StmtKind::Return { .. }) {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::RET505) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::SuperfluousElseReturn(branch),
                    Range::from_located(stmt),
                ));
            }
            return true;
        }
        if matches!(child.node, StmtKind::Break) {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::RET508) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::SuperfluousElseBreak(branch),
                    Range::from_located(stmt),
                ));
            }
            return true;
        }
        if matches!(child.node, StmtKind::Raise { .. }) {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::RET506) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::SuperfluousElseRaise(branch),
                    Range::from_located(stmt),
                ));
            }
            return true;
        }
        if matches!(child.node, StmtKind::Continue) {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::RET507) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::SuperfluousElseContinue(branch),
                    Range::from_located(stmt),
                ));
            }
            return true;
        }
    }
    false
}

/// RET505, RET506, RET507, RET508
fn superfluous_elif(xxxxxxxx: &mut xxxxxxxx, stack: &Stack) -> bool {
    for stmt in &stack.elifs {
        if superfluous_else_node(xxxxxxxx, stmt, Branch::Elif) {
            return true;
        }
    }
    false
}

/// RET505, RET506, RET507, RET508
fn superfluous_else(xxxxxxxx: &mut xxxxxxxx, stack: &Stack) -> bool {
    for stmt in &stack.ifs {
        let StmtKind::If { orelse, .. } = &stmt.node else {
            continue;
        };
        if orelse.is_empty() {
            continue;
        }
        if superfluous_else_node(xxxxxxxx, stmt, Branch::Else) {
            return true;
        }
    }
    false
}

/// Run all checks from the `flake8-return` plugin.
pub fn function(xxxxxxxx: &mut xxxxxxxx, body: &[Stmt]) {
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

    if xxxxxxxx.settings.enabled.contains(&RuleCode::RET505)
        || xxxxxxxx.settings.enabled.contains(&RuleCode::RET506)
        || xxxxxxxx.settings.enabled.contains(&RuleCode::RET507)
        || xxxxxxxx.settings.enabled.contains(&RuleCode::RET508)
    {
        if superfluous_elif(xxxxxxxx, &stack) {
            return;
        }
        if superfluous_else(xxxxxxxx, &stack) {
            return;
        }
    }

    // Skip any functions without return statements.
    if stack.returns.is_empty() {
        return;
    }

    if !result_exists(&stack.returns) {
        if xxxxxxxx.settings.enabled.contains(&RuleCode::RET501) {
            unnecessary_return_none(xxxxxxxx, &stack);
        }
        return;
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::RET502) {
        implicit_return_value(xxxxxxxx, &stack);
    }
    if xxxxxxxx.settings.enabled.contains(&RuleCode::RET503) {
        implicit_return(xxxxxxxx, last_stmt);
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::RET504) {
        for (_, expr) in &stack.returns {
            if let Some(expr) = expr {
                unnecessary_assign(xxxxxxxx, &stack, expr);
            }
        }
    }
}
