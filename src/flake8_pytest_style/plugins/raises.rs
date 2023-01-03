use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword, Stmt, StmtKind, Withitem};

use super::helpers::is_empty_or_null_string;
use crate::ast::helpers::{
    collect_call_paths, dealias_call_path, match_call_path, match_module_member,
    to_module_and_member,
};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};

fn is_pytest_raises(
    func: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    match_module_member(func, "pytest", "raises", from_imports, import_aliases)
}

fn is_non_trivial_with_body(body: &Vec<Stmt>) -> bool {
    if body.len() > 1 {
        true
    } else if let Some(first_body_stmt) = body.first() {
        !matches!(first_body_stmt.node, StmtKind::Pass)
    } else {
        false
    }
}

pub fn raises_call(checker: &mut Checker, func: &Expr, args: &Vec<Expr>, keywords: &Vec<Keyword>) {
    if is_pytest_raises(func, &checker.from_imports, &checker.import_aliases) {
        if checker.settings.enabled.contains(&CheckCode::PT010) {
            if args.is_empty() && keywords.is_empty() {
                checker.add_check(Check::new(
                    CheckKind::RaisesWithoutException,
                    Range::from_located(func),
                ));
            }
        }

        if checker.settings.enabled.contains(&CheckCode::PT011) {
            let match_keyword = keywords
                .iter()
                .find(|kw| kw.node.arg == Some("match".to_string()));

            if let Some(exception) = args.first() {
                if let Some(match_keyword) = match_keyword {
                    if is_empty_or_null_string(&match_keyword.node.value) {
                        exception_needs_match(checker, exception);
                    }
                } else {
                    exception_needs_match(checker, exception);
                }
            }
        }
    }
}

pub fn complex_raises(checker: &mut Checker, stmt: &Stmt, items: &[Withitem], body: &Vec<Stmt>) {
    let mut is_too_complex = false;

    let raises_called = items.iter().any(|item| match &item.context_expr.node {
        ExprKind::Call { func, .. } => {
            is_pytest_raises(func, &checker.from_imports, &checker.import_aliases)
        }
        _ => false,
    });

    // Check body for `pytest.raises` context manager
    if raises_called {
        if body.len() > 1 {
            is_too_complex = true;
        } else if let Some(first_stmt) = body.first() {
            match &first_stmt.node {
                StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => {
                    if is_non_trivial_with_body(body) {
                        is_too_complex = true;
                    }
                }
                StmtKind::If { .. }
                | StmtKind::For { .. }
                | StmtKind::AsyncFor { .. }
                | StmtKind::While { .. }
                | StmtKind::Try { .. } => {
                    is_too_complex = true;
                }
                _ => {}
            }
        }

        if is_too_complex {
            checker.add_check(Check::new(
                CheckKind::RaisesWithMultipleStatements,
                Range::from_located(stmt),
            ));
        }
    }
}

/// PT011
fn exception_needs_match(checker: &mut Checker, exception: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(exception), &checker.import_aliases);

    let is_broad_exception = checker
        .settings
        .flake8_pytest_style
        .raises_require_match_for
        .iter()
        .chain(
            &checker
                .settings
                .flake8_pytest_style
                .raises_extend_require_match_for,
        )
        .map(|target| to_module_and_member(target))
        .any(|(module, member)| match_call_path(&call_path, module, member, &checker.from_imports));

    if is_broad_exception {
        checker.add_check(Check::new(
            CheckKind::RaisesTooBroad(call_path.join(".")),
            Range::from_located(exception),
        ));
    }
}
