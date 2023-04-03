use rustpython_parser::ast::{Expr, ExprKind, Keyword, Stmt, StmtKind, Withitem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::format_call_path;
use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::helpers::is_empty_or_null_string;

#[violation]
pub struct PytestRaisesWithMultipleStatements;

impl Violation for PytestRaisesWithMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pytest.raises()` block should contain a single simple statement")
    }
}

#[violation]
pub struct PytestRaisesTooBroad {
    pub exception: String,
}

impl Violation for PytestRaisesTooBroad {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestRaisesTooBroad { exception } = self;
        format!(
            "`pytest.raises({exception})` is too broad, set the `match` parameter or use a more \
             specific exception"
        )
    }
}

#[violation]
pub struct PytestRaisesWithoutException;

impl Violation for PytestRaisesWithoutException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("set the expected exception in `pytest.raises()`")
    }
}

fn is_pytest_raises(checker: &Checker, func: &Expr) -> bool {
    checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "raises"]
        })
}

const fn is_non_trivial_with_body(body: &[Stmt]) -> bool {
    if body.len() > 1 {
        true
    } else if let Some(first_body_stmt) = body.first() {
        !matches!(first_body_stmt.node, StmtKind::Pass)
    } else {
        false
    }
}

pub fn raises_call(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if is_pytest_raises(checker, func) {
        if checker
            .settings
            .rules
            .enabled(Rule::PytestRaisesWithoutException)
        {
            if args.is_empty() && keywords.is_empty() {
                checker.diagnostics.push(Diagnostic::new(
                    PytestRaisesWithoutException,
                    Range::from(func),
                ));
            }
        }

        if checker.settings.rules.enabled(Rule::PytestRaisesTooBroad) {
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

pub fn complex_raises(checker: &mut Checker, stmt: &Stmt, items: &[Withitem], body: &[Stmt]) {
    let mut is_too_complex = false;

    let raises_called = items.iter().any(|item| match &item.context_expr.node {
        ExprKind::Call { func, .. } => is_pytest_raises(checker, func),
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
                | StmtKind::Match { .. }
                | StmtKind::AsyncFor { .. }
                | StmtKind::While { .. }
                | StmtKind::Try { .. }
                | StmtKind::TryStar { .. } => {
                    is_too_complex = true;
                }
                _ => {}
            }
        }

        if is_too_complex {
            checker.diagnostics.push(Diagnostic::new(
                PytestRaisesWithMultipleStatements,
                Range::from(stmt),
            ));
        }
    }
}

/// PT011
fn exception_needs_match(checker: &mut Checker, exception: &Expr) {
    if let Some(call_path) = checker
        .ctx
        .resolve_call_path(exception)
        .and_then(|call_path| {
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
                .any(|target| call_path == from_qualified_name(target));
            if is_broad_exception {
                Some(format_call_path(&call_path))
            } else {
                None
            }
        })
    {
        checker.diagnostics.push(Diagnostic::new(
            PytestRaisesTooBroad {
                exception: call_path,
            },
            Range::from(exception),
        ));
    }
}
