use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_compound_statement;
use ruff_python_ast::{self as ast, Expr, Stmt, WithItem};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::helpers::is_empty_or_null_string;

/// ## What it does
/// Checks for `pytest.warns` context managers with multiple statements.
///
/// This rule allows `pytest.warns` bodies to contain `for`
/// loops with empty bodies (e.g., `pass` or `...` statements), to test
/// iterator behavior.
///
/// ## Why is this bad?
/// When `pytest.warns` is used as a context manager and contains multiple
/// statements, it can lead to the test passing when it should instead fail.
///
/// A `pytest.warns` context manager should only contain a single
/// simple statement that triggers the expected warning.
///
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo_warns():
///     with pytest.warns(Warning):
///         setup()  # False negative if setup triggers a warning but foo does not.
///         foo()
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo_warns():
///     setup()
///     with pytest.warns(Warning):
///         foo()
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.warns`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-warns)
#[derive(ViolationMetadata)]
pub(crate) struct PytestWarnsWithMultipleStatements;

impl Violation for PytestWarnsWithMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`pytest.warns()` block should contain a single simple statement".to_string()
    }
}

/// ## What it does
/// Checks for `pytest.warns` calls without a `match` parameter.
///
/// ## Why is this bad?
/// `pytest.warns(Warning)` will catch any `Warning` and may catch warnings that
/// are unrelated to the code under test. To avoid this, `pytest.warns` should
/// be called with a `match` parameter. The warning names that require a `match`
/// parameter can be configured via the
/// [`lint.flake8-pytest-style.warns-require-match-for`] and
/// [`lint.flake8-pytest-style.warns-extend-require-match-for`] settings.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.warns(RuntimeWarning):
///         ...
///
///     # empty string is also an error
///     with pytest.warns(RuntimeWarning, match=""):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.warns(RuntimeWarning, match="expected message"):
///         ...
/// ```
///
/// ## Options
/// - `lint.flake8-pytest-style.warns-require-match-for`
/// - `lint.flake8-pytest-style.warns-extend-require-match-for`
///
/// ## References
/// - [`pytest` documentation: `pytest.warns`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-warns)
#[derive(ViolationMetadata)]
pub(crate) struct PytestWarnsTooBroad {
    warning: String,
}

impl Violation for PytestWarnsTooBroad {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestWarnsTooBroad { warning } = self;
        format!(
            "`pytest.warns({warning})` is too broad, set the `match` parameter or use a more \
             specific warning"
        )
    }
}

/// ## What it does
/// Checks for `pytest.warns` calls without an expected warning.
///
/// ## Why is this bad?
/// `pytest.warns` expects to receive an expected warning as its first
/// argument. If omitted, the `pytest.warns` call will fail at runtime.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.warns():
///         do_something()
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.warns(SomeWarning):
///         do_something()
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.warns`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-warns)
#[derive(ViolationMetadata)]
pub(crate) struct PytestWarnsWithoutWarning;

impl Violation for PytestWarnsWithoutWarning {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Set the expected warning in `pytest.warns()`".to_string()
    }
}

pub(crate) fn is_pytest_warns(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "warns"]))
}

const fn is_non_trivial_with_body(body: &[Stmt]) -> bool {
    if let [stmt] = body {
        is_compound_statement(stmt)
    } else {
        true
    }
}

/// PT029, PT030
pub(crate) fn warns_call(checker: &Checker, call: &ast::ExprCall) {
    if is_pytest_warns(&call.func, checker.semantic()) {
        if checker.enabled(Rule::PytestWarnsWithoutWarning) {
            if call.arguments.is_empty() {
                checker.report_diagnostic(Diagnostic::new(
                    PytestWarnsWithoutWarning,
                    call.func.range(),
                ));
            }
        }

        if checker.enabled(Rule::PytestWarnsTooBroad) {
            if let Some(warning) = call.arguments.find_argument_value("expected_warning", 0) {
                if call
                    .arguments
                    .find_keyword("match")
                    .is_none_or(|k| is_empty_or_null_string(&k.value))
                {
                    warning_needs_match(checker, warning);
                }
            }
        }
    }
}

/// PT031
pub(crate) fn complex_warns(checker: &Checker, stmt: &Stmt, items: &[WithItem], body: &[Stmt]) {
    let warns_called = items.iter().any(|item| match &item.context_expr {
        Expr::Call(ast::ExprCall { func, .. }) => is_pytest_warns(func, checker.semantic()),
        _ => false,
    });

    // Check body for `pytest.warns` context manager
    if warns_called {
        let is_too_complex = if let [stmt] = body {
            match stmt {
                Stmt::With(ast::StmtWith { body, .. }) => is_non_trivial_with_body(body),
                // Allow function and class definitions to test decorators.
                Stmt::ClassDef(_) | Stmt::FunctionDef(_) => false,
                // Allow empty `for` loops to test iterators.
                Stmt::For(ast::StmtFor { body, .. }) => match &body[..] {
                    [Stmt::Pass(_)] => false,
                    [Stmt::Expr(ast::StmtExpr { value, .. })] => !value.is_ellipsis_literal_expr(),
                    _ => true,
                },
                stmt => is_compound_statement(stmt),
            }
        } else {
            true
        };

        if is_too_complex {
            checker.report_diagnostic(Diagnostic::new(
                PytestWarnsWithMultipleStatements,
                stmt.range(),
            ));
        }
    }
}

/// PT030
fn warning_needs_match(checker: &Checker, warning: &Expr) {
    if let Some(qualified_name) =
        checker
            .semantic()
            .resolve_qualified_name(warning)
            .and_then(|qualified_name| {
                let qualified_name = qualified_name.to_string();
                checker
                    .settings
                    .flake8_pytest_style
                    .warns_require_match_for
                    .iter()
                    .chain(
                        &checker
                            .settings
                            .flake8_pytest_style
                            .warns_extend_require_match_for,
                    )
                    .any(|pattern| pattern.matches(&qualified_name))
                    .then_some(qualified_name)
            })
    {
        checker.report_diagnostic(Diagnostic::new(
            PytestWarnsTooBroad {
                warning: qualified_name,
            },
            warning.range(),
        ));
    }
}
