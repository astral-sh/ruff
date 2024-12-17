use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_compound_statement;
use ruff_python_ast::{self as ast, Expr, Keyword, Stmt, StringLiteralValue, WithItem};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use super::helpers::is_empty_or_null_string;
use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for `pytest.raises` context managers with multiple statements.
///
/// ## Why is this bad?
/// When a `pytest.raises` is used as a context manager and contains multiple
/// statements, it can lead to the test passing when it actually should fail.
/// To avoid this, a `pytest.raises` context manager should only contain
/// a single simple statement that raises the expected exception.
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
        "set the expected exception in `pytest.raises()`".to_string()
    }
}

/// ## What it does
/// Checks for non-raw literal string arguments passed to the `match` parameter
/// of `pytest.raises()` where the string contains at least one unescaped
/// regex metacharacter.
///
/// ## Why is this bad?
/// The `match` argument is implicitly converted to a regex under the hood.
/// It should be made explicit whether the string is meant to be a regex or a "plain" pattern
/// by prefixing the string with the `r` suffix, escaping the metacharacter(s)
/// in the string using backslashes, or wrapping the entire string in a call to
/// `re.escape()`.
///
/// ## Example
///
/// ```python
/// import pytest
///
///
/// with pytest.raises(Exception, match="A full sentence."):
///     do_thing_that_raises()
/// ```
///
/// Use instead:
///
/// ```python
/// import pytest
///
///
/// with pytest.raises(Exception, match=r"A full sentence."):
///     do_thing_that_raises()
/// ```
///
/// Alternatively:
///
/// ```python
/// import pytest
/// import re
///
///
/// with pytest.raises(Exception, match=re.escape("A full sentence.")):
///     do_thing_that_raises()
/// ```
///
/// or:
///
/// ```python
/// import pytest
/// import re
///
///
/// with pytest.raises(Exception, "A full sentence\\."):
///     do_thing_that_raises()
/// ```
///
/// ## References
/// - [Python documentation: `re.escape`](https://docs.python.org/3/library/re.html#re.escape)
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[derive(ViolationMetadata)]
pub(crate) struct PytestRaisesAmbiguousPattern;

impl Violation for PytestRaisesAmbiguousPattern {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Pattern passed to `match=` contains metacharacters but is neither escaped nor raw"
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use a raw string or `re.escape()` to make the intention explicit".to_string())
    }
}

fn is_pytest_raises(func: &Expr, semantic: &SemanticModel) -> bool {
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

fn string_has_unescaped_metacharacters(value: &StringLiteralValue) -> bool {
    let mut escaped = false;
    let mut last = '\x00';

    for (current, next) in value.chars().tuple_windows() {
        last = next;

        if escaped {
            if escaped_char_is_regex_metasequence(current) {
                return true;
            }

            escaped = false;
            continue;
        }

        if current == '\\' {
            escaped = true;
            continue;
        }

        if char_is_regex_metacharacter(current) {
            return true;
        }
    }

    if escaped {
        escaped_char_is_regex_metasequence(last)
    } else {
        char_is_regex_metacharacter(last)
    }
}

/// Whether the sequence `\<c>` means anything special:
///
/// * `\A`: Start of input
/// * `\b`, `\B`: Word boundary and non-word-boundary
/// * `\d`, `\D`: Digit and non-digit
/// * `\s`, `\S`: Whitespace and non-whitespace
/// * `\w`, `\W`: Word and non-word character
/// * `\z`: End of input
///
/// `\u`, `\U`, `\N`, `\x`, `\a`, `\f`, `\n`, `\r`, `\t`, `\v`
/// are also valid in normal strings and thus do not count.
/// `\b` means backspace only in character sets,
/// while backreferences (e.g., `\1`) are not valid without groups,
/// both of which should be caught in [`string_has_unescaped_metacharacters`].
const fn escaped_char_is_regex_metasequence(c: char) -> bool {
    matches!(c, 'A' | 'b' | 'B' | 'd' | 'D' | 's' | 'S' | 'w' | 'W' | 'z')
}

const fn char_is_regex_metacharacter(c: char) -> bool {
    matches!(
        c,
        '.' | '^' | '$' | '*' | '+' | '?' | '{' | '[' | '\\' | '|' | '('
    )
}

pub(crate) fn raises_call(checker: &mut Checker, call: &ast::ExprCall) {
    if is_pytest_raises(&call.func, checker.semantic()) {
        if checker.enabled(Rule::PytestRaisesWithoutException) {
            if call.arguments.is_empty() {
                checker.diagnostics.push(Diagnostic::new(
                    PytestRaisesWithoutException,
                    call.func.range(),
                ));
            }
        }

        if checker.enabled(Rule::PytestRaisesAmbiguousPattern) {
            if let Some(Keyword { value, .. }) = call.arguments.find_keyword("match") {
                if let Some(string) = value.as_string_literal_expr() {
                    let any_part_is_raw =
                        string.value.iter().any(|part| part.flags.prefix().is_raw());

                    if !any_part_is_raw && string_has_unescaped_metacharacters(&string.value) {
                        let diagnostic =
                            Diagnostic::new(PytestRaisesAmbiguousPattern, string.range);
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }

        if checker.enabled(Rule::PytestRaisesTooBroad) {
            if let Some(exception) = call.arguments.find_argument("expected_exception", 0) {
                if call
                    .arguments
                    .find_keyword("match")
                    .map_or(true, |k| is_empty_or_null_string(&k.value))
                {
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
                Stmt::With(ast::StmtWith { body, .. }) => is_non_trivial_with_body(body),
                // Allow function and class definitions to test decorators
                Stmt::ClassDef(_) | Stmt::FunctionDef(_) => false,
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
        checker.diagnostics.push(Diagnostic::new(
            PytestRaisesTooBroad {
                exception: qualified_name,
            },
            exception.range(),
        ));
    }
}
