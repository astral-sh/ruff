use log::error;
use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::{BindingKind, RefEquality, ScopeKind};
use crate::autofix::helpers::delete_stmt;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};

fn is_literal_or_name(expr: &Expr) -> bool {
    // Accept any obvious literals or names.
    if matches!(
        expr.node,
        ExprKind::Constant { .. }
            | ExprKind::Name { .. }
            | ExprKind::List { .. }
            | ExprKind::Tuple { .. }
            | ExprKind::Set { .. }
    ) {
        return true;
    }

    // Accept empty initializers.
    if let ExprKind::Call {
        func,
        args,
        keywords,
    } = &expr.node
    {
        if args.is_empty() && keywords.is_empty() {
            if let ExprKind::Name { id, .. } = &func.node {
                return id == "set"
                    || id == "list"
                    || id == "tuple"
                    || id == "dict"
                    || id == "frozenset";
            }
        }
    }

    false
}

enum DeletionKind {
    Whole,
    Partial,
}

fn remove_unused_variable(stmt: &Stmt, checker: &Checker) -> Option<(DeletionKind, Fix)> {
    // First case: simple assignment (`x = 1`)
    // TODO(charlie): For tuple assignments, we can replace names with underscores.
    // TODO(charlie): For context managers, we can get rid of the `as` clause.
    if let StmtKind::Assign { targets, value, .. } = &stmt.node {
        if targets.len() == 1 {
            if matches!(targets[0].node, ExprKind::Name { .. }) {
                return if is_literal_or_name(value) {
                    // If assigning to a constant (`x = 1`), delete the entire statement.
                    let parent = checker
                        .child_to_parent
                        .get(&RefEquality(stmt))
                        .map(|parent| parent.0);
                    let deleted: Vec<&Stmt> = checker.deletions.iter().map(|node| node.0).collect();
                    let locator = checker.locator;
                    match delete_stmt(stmt, parent, &deleted, locator) {
                        Ok(fix) => Some((DeletionKind::Whole, fix)),
                        Err(err) => {
                            error!("Failed to delete unused variable: {}", err);
                            None
                        }
                    }
                } else {
                    // If the expression is more complex (`x = foo()`), remove the assignment,
                    // but preserve the right-hand side.
                    Some((
                        DeletionKind::Partial,
                        Fix::deletion(stmt.location, value.location),
                    ))
                };
            }
        }
    }

    // Second case: simple annotated assignment (`x: int = 1`)
    if let StmtKind::AnnAssign { target, value, .. } = &stmt.node {
        if let Some(value) = value {
            if matches!(targets[0].node, ExprKind::Name { .. }) {
                return if is_literal_or_name(value) {
                    // If assigning to a constant (`x = 1`), delete the entire statement.
                    let parent = checker
                        .child_to_parent
                        .get(&RefEquality(stmt))
                        .map(|parent| parent.0);
                    let deleted: Vec<&Stmt> = checker.deletions.iter().map(|node| node.0).collect();
                    let locator = checker.locator;
                    match delete_stmt(stmt, parent, &deleted, locator) {
                        Ok(fix) => Some((DeletionKind::Whole, fix)),
                        Err(err) => {
                            error!("Failed to delete unused variable: {}", err);
                            None
                        }
                    }
                } else {
                    // If the expression is more complex (`x = foo()`), remove the assignment,
                    // but preserve the right-hand side.
                    Some((
                        DeletionKind::Partial,
                        Fix::deletion(stmt.location, value.location),
                    ))
                };
            }
        }
    }

    None
}

/// F841
pub fn unused_variable(checker: &mut Checker, scope: usize) {
    let scope = &checker.scopes[scope];
    if scope.uses_locals && matches!(scope.kind, ScopeKind::Function(..)) {
        return;
    }

    // TODO(charlie): It's really bad that this check is reaching into so much
    // internal `Checker` state.
    let mut checks = vec![];
    for (name, binding) in scope
        .values
        .iter()
        .map(|(name, index)| (name, &checker.bindings[*index]))
    {
        if binding.used.is_none()
            && matches!(binding.kind, BindingKind::Assignment)
            && !checker.settings.dummy_variable_rgx.is_match(name)
            && name != &"__tracebackhide__"
            && name != &"__traceback_info__"
            && name != &"__traceback_supplement__"
        {
            let mut check = Check::new(
                CheckKind::UnusedVariable((*name).to_string()),
                binding.range,
            );
            if checker.patch(&CheckCode::F841) {
                if let Some(stmt) = binding.source.as_ref().map(|source| source.0) {
                    if let Some((kind, fix)) = remove_unused_variable(stmt, checker) {
                        if matches!(kind, DeletionKind::Whole) {
                            checker.deletions.insert(RefEquality(stmt));
                        }
                        check.amend(fix);
                    }
                }
            }
            checks.push(check);
        }
    }
    checker.add_checks(checks.into_iter());
}
