use ruff_python_ast::helpers::{find_keyword, is_compound_statement};
use ruff_python_ast::{self as ast, Expr, Keyword, Ranged, Stmt, WithItem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::format_call_path;
use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_semantic::SemanticModel;

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
    exception: String,
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

/// ## What it does
/// Checks for `pytest.raises` calls without an expected exception.
///
/// ## Why is this bad?
/// `pytest.raises` expects to receive an expected exception as its first
/// argument. If omitted, the `pytest.raises` call will fail at runtime.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.raises():
///         do_something()
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.raises(SomeException):
///         do_something()
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[violation]
pub struct PytestRaisesWithoutException;

impl Violation for PytestRaisesWithoutException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("set the expected exception in `pytest.raises()`")
    }
}

fn is_pytest_raises(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(call_path.as_slice(), ["pytest", "raises"])
    })
}

const fn is_non_trivial_with_body(body: &[Stmt]) -> bool {
    if let [stmt] = body {
        is_compound_statement(stmt)
    } else {
        true
    }
}

pub(crate) fn raises_call(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if is_pytest_raises(func, checker.semantic()) {
        if checker.enabled(Rule::PytestRaisesWithoutException) {
            if args.is_empty() && keywords.is_empty() {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PytestRaisesWithoutException, func.range()));
            }
        }

        if checker.enabled(Rule::PytestRaisesTooBroad) {
            let match_keyword = find_keyword(keywords, "match");
            if let Some(exception) = args.first() {
                if let Some(match_keyword) = match_keyword {
                    if is_empty_or_null_string(&match_keyword.value) {
                        exception_needs_match(checker, exception);
                    }
                } else {
                    exception_needs_match(checker, exception);
                }
            }
        }
    }
}

pub(crate) fn complex_raises(
    checker: &mut Checker,
    stmt: &Stmt,
    items: &[WithItem],
    body: &[Stmt],
) {
    let raises_called = items.iter().any(|item| match &item.context_expr {
        Expr::Call(ast::ExprCall { func, .. }) => is_pytest_raises(func, checker.semantic()),
        _ => false,
    });

    // Check body for `pytest.raises` context manager
    if raises_called {
        let is_too_complex = if let [stmt] = body {
            match stmt {
                Stmt::With(ast::StmtWith { body, .. })
                | Stmt::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
                    is_non_trivial_with_body(body)
                }
                // Allow function and class definitions to test decorators
                Stmt::ClassDef(_) | Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) => false,
                stmt => is_compound_statement(stmt),
            }
        } else {
            true
        };

        if is_too_complex {
            checker.diagnostics.push(Diagnostic::new(
                PytestRaisesWithMultipleStatements,
                stmt.range(),
            ));
        }
    }
}

/// PT011
fn exception_needs_match(checker: &mut Checker, exception: &Expr) {
    if let Some(call_path) = checker
        .semantic()
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
            exception.range(),
        ));
    }
}
