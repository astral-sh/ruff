use log::error;
use rustc_hash::FxHashSet;
use rustpython_ast::{Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::comparable::ComparableExpr;
use crate::ast::helpers::unparse_expr;
use crate::ast::types::{Range, RefEquality};
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::python::identifiers::is_identifier;
use crate::registry::{Diagnostic, Rule};
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
                if checker.patch(&Rule::NoUnnecessaryPass) {
                    match delete_stmt(pass_stmt, None, &[], checker.locator, checker.indexer) {
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
            if checker.patch(&Rule::DupeClassFieldDefinitions) {
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(std::convert::Into::into)
                    .collect();
                let locator = checker.locator;
                match delete_stmt(stmt, Some(parent), &deleted, locator, checker.indexer) {
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

/// PIE796
pub fn prefer_unique_enums<'a, 'b>(checker: &mut Checker<'a>, parent: &'b Stmt, body: &'b [Stmt])
where
    'b: 'a,
{
    let StmtKind::ClassDef { bases, .. } = &parent.node else {
        return;
    };

    if !bases.iter().any(|expr| {
        checker
            .resolve_call_path(expr)
            .map_or(false, |call_path| call_path.as_slice() == ["enum", "Enum"])
    }) {
        return;
    }

    let mut seen_targets: FxHashSet<ComparableExpr> = FxHashSet::default();
    for stmt in body {
        let StmtKind::Assign { value, .. } = &stmt.node else {
            continue;
        };

        if let ExprKind::Call { func, .. } = &value.node {
            if checker
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path.as_slice() == ["enum", "auto"])
            {
                continue;
            }
        }

        if !seen_targets.insert(ComparableExpr::from(value)) {
            let diagnostic = Diagnostic::new(
                violations::PreferUniqueEnums {
                    value: unparse_expr(value, checker.stylist),
                },
                Range::from_located(stmt),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE800
pub fn no_unnecessary_spread(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    for item in keys.iter().zip(values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let ExprKind::Dict { .. } = value.node {
                let diagnostic =
                    Diagnostic::new(violations::NoUnnecessarySpread, Range::from_located(value));
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// Return `true` if a key is a valid keyword argument name.
fn is_valid_kwarg_name(key: &Expr) -> bool {
    if let ExprKind::Constant {
        value: Constant::Str(value),
        ..
    } = &key.node
    {
        is_identifier(value)
    } else {
        false
    }
}

/// PIE804
pub fn no_unnecessary_dict_kwargs(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    for kw in kwargs {
        // keyword is a spread operator (indicated by None)
        if kw.node.arg.is_none() {
            if let ExprKind::Dict { keys, .. } = &kw.node.value.node {
                // ensure foo(**{"bar-bar": 1}) doesn't error
                if keys.iter().all(|expr| expr.as_ref().map_or(false, is_valid_kwarg_name)) ||
                    // handle case of foo(**{**bar})
                    (keys.len() == 1 && keys[0].is_none())
                {
                    let diagnostic = Diagnostic::new(
                        violations::NoUnnecessaryDictKwargs,
                        Range::from_located(expr),
                    );
                    checker.diagnostics.push(diagnostic);
                }
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
                if checker.patch(&Rule::PreferListBuiltin) {
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
