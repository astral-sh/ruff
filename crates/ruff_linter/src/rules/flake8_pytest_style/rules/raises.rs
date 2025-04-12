use anyhow::{bail, Context};
use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_compound_statement;
use ruff_python_ast::{self as ast, Expr, Stmt, StmtExpr, StmtWith, WithItem};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::helpers::is_empty_or_null_string;

/// ## What it does
/// Checks for `pytest.raises` context managers with multiple statements.
///
/// This rule allows `pytest.raises` bodies to contain `for`
/// loops with empty bodies (e.g., `pass` or `...` statements), to test
/// iterator behavior.
///
/// ## Why is this bad?
/// When a `pytest.raises` is used as a context manager and contains multiple
/// statements, it can lead to the test passing when it actually should fail.
///
/// A `pytest.raises` context manager should only contain a single simple
/// statement that raises the expected exception.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.raises(MyError):
///         setup()
///         func_to_test()  # not executed if `setup()` raises `MyError`
///         assert foo()  # not executed
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     setup()
///     with pytest.raises(MyError):
///         func_to_test()
///     assert foo()
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[derive(ViolationMetadata)]
pub(crate) struct PytestRaisesWithMultipleStatements;

impl Violation for PytestRaisesWithMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`pytest.raises()` block should contain a single simple statement".to_string()
    }
}

/// ## What it does
/// Checks for `pytest.raises` calls without a `match` parameter.
///
/// ## Why is this bad?
/// `pytest.raises(Error)` will catch any `Error` and may catch errors that are
/// unrelated to the code under test. To avoid this, `pytest.raises` should be
/// called with a `match` parameter. The exception names that require a `match`
/// parameter can be configured via the
/// [`lint.flake8-pytest-style.raises-require-match-for`] and
/// [`lint.flake8-pytest-style.raises-extend-require-match-for`] settings.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.raises(ValueError):
///         ...
///
///     # empty string is also an error
///     with pytest.raises(ValueError, match=""):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.raises(ValueError, match="expected message"):
///         ...
/// ```
///
/// ## Options
/// - `lint.flake8-pytest-style.raises-require-match-for`
/// - `lint.flake8-pytest-style.raises-extend-require-match-for`
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[derive(ViolationMetadata)]
pub(crate) struct PytestRaisesTooBroad {
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
#[derive(ViolationMetadata)]
pub(crate) struct PytestRaisesWithoutException;

impl Violation for PytestRaisesWithoutException {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Set the expected exception in `pytest.raises()`".to_string()
    }
}

/// ## What it does
/// Checks for non-contextmanager use of `pytest.raises`.
///
/// ## Why is this bad?
/// The context-manager form is more readable, easier to extend, and supports additional kwargs.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// excinfo = pytest.raises(ValueError, int, "hello")
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// with pytest.raises(ValueError) as excinfo:
///     int("hello")
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[derive(ViolationMetadata)]
pub(crate) struct DeprecatedPytestRaisesCallableForm;

impl Violation for DeprecatedPytestRaisesCallableForm {
    const FIX_AVAILABILITY: ruff_diagnostics::FixAvailability =
        ruff_diagnostics::FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use context-manager form of `pytest.raises()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Rewrite `pytest.raises()` in a context-manager form".to_string())
    }
}

pub(crate) fn is_pytest_raises(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "raises"]))
}

const fn is_non_trivial_with_body(body: &[Stmt]) -> bool {
    if let [stmt] = body {
        is_compound_statement(stmt)
    } else {
        true
    }
}

pub(crate) fn raises_call(checker: &Checker, call: &ast::ExprCall) {
    if is_pytest_raises(&call.func, checker.semantic()) {
        if checker.enabled(Rule::PytestRaisesWithoutException) {
            if call
                .arguments
                .find_argument("expected_exception", 0)
                .is_none()
            {
                checker.report_diagnostic(Diagnostic::new(
                    PytestRaisesWithoutException,
                    call.func.range(),
                ));
            }
        }

        if checker.enabled(Rule::DeprecatedPytestRaisesCallableForm)
            && call.arguments.find_argument("func", 1).is_some()
        {
            let mut diagnostic = Diagnostic::new(DeprecatedPytestRaisesCallableForm, call.range());
            let stmt = checker.semantic().current_statement();
            if !has_leading_content(stmt.start(), checker.source())
                && !has_trailing_content(stmt.end(), checker.source())
            {
                let generated = try_fix_legacy_raises(stmt, checker.semantic()).map(|with| {
                    let generated = checker.generator().stmt(&Stmt::With(with));
                    let first_line = checker.locator().line_str(stmt.start());
                    let indentation = leading_indentation(first_line);
                    let mut indented = String::new();
                    for (idx, line) in generated.universal_newlines().enumerate() {
                        if idx == 0 {
                            indented.push_str(&line);
                        } else {
                            indented.push_str(checker.stylist().line_ending().as_str());
                            indented.push_str(indentation);
                            indented.push_str(&line);
                        }
                    }
                    indented
                });
                diagnostic.try_set_fix(|| {
                    Ok(Fix::unsafe_edit(Edit::range_replacement(
                        generated?,
                        stmt.range(),
                    )))
                });
            }
            checker.report_diagnostic(diagnostic);
        }

        if checker.enabled(Rule::PytestRaisesTooBroad) {
            // Pytest.raises has two overloads
            // ```py
            // with raises(expected_exception: type[E] | tuple[type[E], ...], *, match: str | Pattern[str] | None = ...) → RaisesContext[E] as excinfo
            // with raises(expected_exception: type[E] | tuple[type[E], ...], func: Callable[[...], Any], *args: Any, **kwargs: Any) → ExceptionInfo[E] as excinfo
            // ```
            // Don't raise this diagnostic if the call matches the second overload (has a second positional argument or an argument named `func`)
            if call.arguments.find_argument("func", 1).is_none() {
                if let Some(exception) = call.arguments.find_argument_value("expected_exception", 0)
                {
                    if call
                        .arguments
                        .find_keyword("match")
                        .is_none_or(|k| is_empty_or_null_string(&k.value))
                    {
                        exception_needs_match(checker, exception);
                    }
                }
            }
        }
    }
}

pub(crate) fn complex_raises(checker: &Checker, stmt: &Stmt, items: &[WithItem], body: &[Stmt]) {
    let raises_called = items.iter().any(|item| match &item.context_expr {
        Expr::Call(ast::ExprCall { func, .. }) => is_pytest_raises(func, checker.semantic()),
        _ => false,
    });

    // Check body for `pytest.raises` context manager
    if raises_called {
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
                PytestRaisesWithMultipleStatements,
                stmt.range(),
            ));
        }
    }
}

/// PT011
fn exception_needs_match(checker: &Checker, exception: &Expr) {
    if let Some(qualified_name) = checker
        .semantic()
        .resolve_qualified_name(exception)
        .and_then(|qualified_name| {
            let qualified_name = qualified_name.to_string();
            checker
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
                .any(|pattern| pattern.matches(&qualified_name))
                .then_some(qualified_name)
        })
    {
        checker.report_diagnostic(Diagnostic::new(
            PytestRaisesTooBroad {
                exception: qualified_name,
            },
            exception.range(),
        ));
    }
}

fn try_fix_legacy_raises(stmt: &Stmt, semantic: &SemanticModel) -> anyhow::Result<StmtWith> {
    match stmt {
        Stmt::Expr(StmtExpr { value, .. }) => {
            let Some(call) = value.as_call_expr() else {
                bail!("Expected call expression")
            };

            if is_pytest_raises(&call.func, semantic) {
                generate_with_raises(call, None, None)
            } else {
                let inner_raises_call = call
                    .func
                    .as_attribute_expr()
                    .filter(|expr_attribute| &expr_attribute.attr == "match")
                    .and_then(|expr_attribute| expr_attribute.value.as_call_expr())
                    .filter(|inner_call| is_pytest_raises(&inner_call.func, semantic))
                    .context("Expected call to `match` on the result of `pytest.raises`")?;
                generate_with_raises(inner_raises_call, call.arguments.args.first(), None)
            }
        }
        Stmt::Assign(ast::StmtAssign {
            range: _,
            targets,
            value,
        }) => {
            let [target] = targets.as_slice() else {
                bail!("Expected one assignment target")
            };

            let raises_call = value
                .as_call_expr()
                .filter(|call| is_pytest_raises(&call.func, semantic))
                .context("Expected call to `pytest.raises`")?;

            let optional_vars = Some(target);
            let match_call = None;
            generate_with_raises(raises_call, match_call, optional_vars)
        }
        _ => bail!("Expected direct call or assign statement"),
    }
}

fn generate_with_raises(
    legacy_raises_call: &ast::ExprCall,
    match_arg: Option<&Expr>,
    optional_vars: Option<&Expr>,
) -> anyhow::Result<StmtWith> {
    let expected_exception = match legacy_raises_call
        .arguments
        .find_argument("expected_exception", 0)
        .context("Expected `expected_exception` argument in the call")?
    {
        ast::ArgOrKeyword::Arg(arg) => arg,
        ast::ArgOrKeyword::Keyword(kw) => &kw.value,
    };

    let func = match legacy_raises_call
        .arguments
        .find_argument("func", 1)
        .context("Expected `func` argument in the call")?
    {
        ast::ArgOrKeyword::Arg(arg) => arg,
        ast::ArgOrKeyword::Keyword(kw) => &kw.value,
    };

    let raises_call = ast::ExprCall {
        range: TextRange::default(),
        func: legacy_raises_call.func.clone(),
        arguments: ast::Arguments {
            range: TextRange::default(),
            args: Box::new([expected_exception.clone()]),
            keywords: match_arg
                .map(|expr| ast::Keyword {
                    // Take range from the original expression so that the keyword
                    // argument is generated after positional arguments
                    range: expr.range(),
                    arg: Some(ast::Identifier::new("match", TextRange::default())),
                    value: expr.clone(),
                })
                .as_slice()
                .into(),
        },
    };

    let func_args = legacy_raises_call
        .arguments
        .args
        .iter()
        .filter(|&arg| arg != expected_exception && arg != func)
        .map(Expr::clone)
        .collect();

    let func_keywords = legacy_raises_call
        .arguments
        .keywords
        .iter()
        .filter(|&keyword| &keyword.value != expected_exception && &keyword.value != func)
        .map(ast::Keyword::clone)
        .collect();

    let func_call = ast::ExprCall {
        range: TextRange::default(),
        func: Box::new(func.clone()),
        arguments: ast::Arguments {
            range: TextRange::default(),
            args: func_args,
            keywords: func_keywords,
        },
    };

    Ok(StmtWith {
        range: TextRange::default(),
        is_async: false,
        items: vec![WithItem {
            range: TextRange::default(),
            context_expr: raises_call.into(),
            optional_vars: optional_vars.map(|var| Box::new(var.clone())),
        }],
        body: vec![Stmt::Expr(StmtExpr {
            range: TextRange::default(),
            value: Box::new(func_call.into()),
        })],
    })
}
