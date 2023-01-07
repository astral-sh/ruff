use log::error;
use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::{BindingKind, Range, RefEquality, ScopeKind};
use crate::autofix::helpers::delete_stmt;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticCode};
use crate::violations;

fn is_literal_or_name(expr: &Expr, checker: &Checker) -> bool {
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
                return (id == "set"
                    || id == "list"
                    || id == "tuple"
                    || id == "dict"
                    || id == "frozenset")
                    && checker.is_builtin(id);
            }
        }
    }

    false
}

enum DeletionKind {
    Whole,
    Partial,
}

/// Generate a `Fix` to remove an unused variable assignment, given the
/// enclosing `Stmt` and the `Range` of the variable binding.
fn remove_unused_variable(
    stmt: &Stmt,
    range: &Range,
    checker: &Checker,
) -> Option<(DeletionKind, Fix)> {
    // First case: simple assignment (`x = 1`)
    if let StmtKind::Assign { targets, value, .. } = &stmt.node {
        if targets.len() == 1 && matches!(targets[0].node, ExprKind::Name { .. }) {
            return if is_literal_or_name(value, checker) {
                // If assigning to a constant (`x = 1`), delete the entire statement.
                let parent = checker
                    .child_to_parent
                    .get(&RefEquality(stmt))
                    .map(std::convert::Into::into);
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(std::convert::Into::into)
                    .collect();
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

    // Second case: simple annotated assignment (`x: int = 1`)
    if let StmtKind::AnnAssign {
        target,
        value: Some(value),
        ..
    } = &stmt.node
    {
        if matches!(target.node, ExprKind::Name { .. }) {
            return if is_literal_or_name(value, checker) {
                // If assigning to a constant (`x = 1`), delete the entire statement.
                let parent = checker
                    .child_to_parent
                    .get(&RefEquality(stmt))
                    .map(std::convert::Into::into);
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(std::convert::Into::into)
                    .collect();
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

    // Third case: withitem (`with foo() as x:`)
    if let StmtKind::With { items, .. } = &stmt.node {
        // Find the binding that matches the given `Range`.
        // TODO(charlie): Store the `Withitem` in the `Binding`.
        for item in items {
            if let Some(optional_vars) = &item.optional_vars {
                if optional_vars.location == range.location
                    && optional_vars.end_location.unwrap() == range.end_location
                {
                    return Some((
                        DeletionKind::Partial,
                        Fix::deletion(
                            item.context_expr.end_location.unwrap(),
                            optional_vars.end_location.unwrap(),
                        ),
                    ));
                }
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
            let mut check = Diagnostic::new(
                violations::UnusedVariable((*name).to_string()),
                binding.range,
            );
            if checker.patch(&DiagnosticCode::F841) {
                if let Some(stmt) = binding.source.as_ref().map(std::convert::Into::into) {
                    if let Some((kind, fix)) = remove_unused_variable(stmt, &binding.range, checker)
                    {
                        if matches!(kind, DeletionKind::Whole) {
                            checker.deletions.insert(RefEquality(stmt));
                        }
                        check.amend(fix);
                    }
                }
            }
            checker.checks.push(check);
        }
    }
}
