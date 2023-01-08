use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword, Stmt, StmtKind, Withitem};

use super::helpers::is_empty_or_null_string;
use crate::ast::helpers::{
    collect_call_paths, dealias_call_path, match_call_path, match_module_member,
    to_module_and_member,
};
use crate::ast::types::Range;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn is_pytest_raises(
    func: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    match_module_member(func, "pytest", "raises", from_imports, import_aliases)
}

fn is_non_trivial_with_body(body: &[Stmt]) -> bool {
    if body.len() > 1 {
        true
    } else if let Some(first_body_stmt) = body.first() {
        !matches!(first_body_stmt.node, StmtKind::Pass)
    } else {
        false
    }
}

pub fn raises_call(xxxxxxxx: &mut xxxxxxxx, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if is_pytest_raises(func, &xxxxxxxx.from_imports, &xxxxxxxx.import_aliases) {
        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT010) {
            if args.is_empty() && keywords.is_empty() {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::RaisesWithoutException,
                    Range::from_located(func),
                ));
            }
        }

        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT011) {
            let match_keyword = keywords
                .iter()
                .find(|kw| kw.node.arg == Some("match".to_string()));

            if let Some(exception) = args.first() {
                if let Some(match_keyword) = match_keyword {
                    if is_empty_or_null_string(&match_keyword.node.value) {
                        exception_needs_match(xxxxxxxx, exception);
                    }
                } else {
                    exception_needs_match(xxxxxxxx, exception);
                }
            }
        }
    }
}

pub fn complex_raises(xxxxxxxx: &mut xxxxxxxx, stmt: &Stmt, items: &[Withitem], body: &[Stmt]) {
    let mut is_too_complex = false;

    let raises_called = items.iter().any(|item| match &item.context_expr.node {
        ExprKind::Call { func, .. } => {
            is_pytest_raises(func, &xxxxxxxx.from_imports, &xxxxxxxx.import_aliases)
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
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::RaisesWithMultipleStatements,
                Range::from_located(stmt),
            ));
        }
    }
}

/// PT011
fn exception_needs_match(xxxxxxxx: &mut xxxxxxxx, exception: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(exception), &xxxxxxxx.import_aliases);

    let is_broad_exception = xxxxxxxx
        .settings
        .flake8_pytest_style
        .raises_require_match_for
        .iter()
        .chain(
            &xxxxxxxx
                .settings
                .flake8_pytest_style
                .raises_extend_require_match_for,
        )
        .map(|target| to_module_and_member(target))
        .any(|(module, member)| {
            match_call_path(&call_path, module, member, &xxxxxxxx.from_imports)
        });

    if is_broad_exception {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::RaisesTooBroad(call_path.join(".")),
            Range::from_located(exception),
        ));
    }
}
