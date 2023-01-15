use log::error;
use rustc_hash::FxHashSet;
use rustpython_ast::{Constant, Expr, ExprKind, Keyword, Located, Stmt, StmtKind};

use crate::ast::types::{Range, RefEquality};
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
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
pub fn dupe_class_field_definitions<'a, 'b>(
    checker: &mut Checker<'a>,
    parent: &'b Stmt,
    body: &'b [Stmt],
) where
    'b: 'a,
{
    let mut seen_targets: FxHashSet<&str> = FxHashSet::default();
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

        if !seen_targets.insert(target) {
            let mut diagnostic = Diagnostic::new(
                violations::DupeClassFieldDefinitions(target.to_string()),
                Range::from_located(stmt),
            );
            if checker.patch(&RuleCode::PIE794) {
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(std::convert::Into::into)
                    .collect();
                let locator = checker.locator;
                match delete_stmt(stmt, Some(parent), &deleted, locator) {
                    Ok(fix) => {
                        checker.deletions.insert(RefEquality(stmt));
                        diagnostic.amend(fix);
                    }
                    Err(err) => {
                        error!("Failed to remove duplicate class definition: {}", err);
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn is_valid_kwarg_name(key: &Located<ExprKind>) -> bool {
    if let ExprKind::Constant {
        value: Constant::Str(key_str),
        ..
    } = &key.node
    {
        // can't have empty keyword args
        if key_str.is_empty() {
            return false;
        }

        // can't start with digit
        if key_str
            .chars()
            .next()
            .map(char::is_numeric)
            .unwrap_or(false)
        {
            return false;
        }

        // only ascii digits, letters, and underscore are allowed in kwargs
        if key_str
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return true;
        }

        // TODO: add check for spread to allow: foo(**{**bar, "buzz": 1})
        // see: https://github.com/RustPython/RustPython/pull/4449

        return false;
    }
    return false;
}

/// PIE804
pub fn no_unnecessary_dict_kwargs(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    kwargs: &[Keyword],
) {
    for kw in kwargs {
        if let ExprKind::Dict { keys, values } = &kw.node.value.node {
            if keys.iter().all(is_valid_kwarg_name) {
                let mut diagnostic = Diagnostic::new(
                    violations::NoUnnecessaryDictKwargs,
                    Range::from_located(expr),
                );
                // if checker.patch(&RuleCode::PIE804) {
                //     diagnostic.amend(Fix::replacement(
                //         "list".to_string(),
                //         expr.location,
                //         expr.end_location.unwrap(),
                //     ));
                // }
                checker.diagnostics.push(diagnostic);
            }
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
