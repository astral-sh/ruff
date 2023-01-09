use log::error;
use rustc_hash::FxHashSet;
use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::helpers::delete_stmt;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;

/// PIE790
pub fn no_unnecessary_pass(checker: &mut Checker, body: &[Stmt]) {
    if body.len() > 1 {
        // This only catches the case in which a docstring makes a `pass` statement
        // redundant. Consider removing all `pass` statements instead.
        let docstring_stmt = &body[0];
        let pass_stmt = &body[1];
        let StmtKind::Expr { value } = &docstring_stmt.node else {
            return;
        };
        if matches!(
            value.node,
            ExprKind::Constant {
                value: Constant::Str(..),
                ..
            }
        ) {
            if matches!(pass_stmt.node, StmtKind::Pass) {
                let mut diagnostic = Diagnostic::new(
                    violations::NoUnnecessaryPass,
                    Range::from_located(pass_stmt),
                );
                if checker.patch(&RuleCode::PIE790) {
                    match delete_stmt(pass_stmt, None, &[], checker.locator) {
                        Ok(fix) => {
                            diagnostic.amend(fix);
                        }
                        Err(e) => {
                            error!("Failed to delete `pass` statement: {}", e);
                        }
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// PIE794
pub fn dupe_class_field_definitions(checker: &mut Checker, bases: &[Expr], body: &[Stmt]) {
    if bases.is_empty() {
        return;
    }

    let mut seen_targets = FxHashSet::default();
    for stmt in body {
        // Extract the property name from the assignment statement.
        let target = match &stmt.node {
            StmtKind::Assign { targets, .. } => {
                if targets.len() != 1 {
                    continue;
                }
                if let ExprKind::Name { id, .. } = &targets[0].node {
                    id
                } else {
                    continue;
                }
            }
            StmtKind::AnnAssign { target, .. } => {
                if let ExprKind::Name { id, .. } = &target.node {
                    id
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        if seen_targets.contains(target) {
            let mut diagnostic = Diagnostic::new(
                violations::DupeClassFieldDefinitions(target.to_string()),
                Range::from_located(stmt),
            );
            if checker.patch(&RuleCode::PIE794) {
                diagnostic.amend(Fix::deletion(stmt.location, stmt.end_location.unwrap()));
            }
            checker.diagnostics.push(diagnostic);
        } else {
            seen_targets.insert(target);
        }
    }
}

/// PIE807
pub fn prefer_list_builtin(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Lambda { args, body } = &expr.node else {
        unreachable!("Expected ExprKind::Lambda");
    };
    if args.args.is_empty() {
        if let ExprKind::List { elts, .. } = &body.node {
            if elts.is_empty() {
                let mut diagnostic =
                    Diagnostic::new(violations::PreferListBuiltin, Range::from_located(expr));
                if checker.patch(&RuleCode::PIE807) {
                    diagnostic.amend(Fix::replacement(
                        "list".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
