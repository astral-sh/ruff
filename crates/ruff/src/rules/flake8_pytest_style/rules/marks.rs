use ruff_python_ast::{self as ast, Arguments, Decorator, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::helpers::get_mark_decorators;

/// ## What it does
/// Checks for argument-free `@pytest.mark.<marker>()` decorators with or
/// without parentheses, depending on the `flake8-pytest-style.mark-parentheses`
/// setting.
///
/// ## Why is this bad?
/// If a `@pytest.mark.<marker>()` doesn't take any arguments, the parentheses are
/// optional.
///
/// Either removing those unnecessary parentheses _or_ requiring them for all
/// fixtures is fine, but it's best to be consistent.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.mark.foo
/// def test_something():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.mark.foo()
/// def test_something():
///     ...
/// ```
///
/// ## Options
/// - `flake8-pytest-style.mark-parentheses`
///
/// ## References
/// - [`pytest` documentation: Marks](https://docs.pytest.org/en/latest/reference/reference.html#marks)
#[violation]
pub struct PytestIncorrectMarkParenthesesStyle {
    mark_name: String,
    expected_parens: String,
    actual_parens: String,
}

impl AlwaysAutofixableViolation for PytestIncorrectMarkParenthesesStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestIncorrectMarkParenthesesStyle {
            mark_name,
            expected_parens,
            actual_parens,
        } = self;
        format!(
            "Use `@pytest.mark.{mark_name}{expected_parens}` over \
             `@pytest.mark.{mark_name}{actual_parens}`"
        )
    }

    fn autofix_title(&self) -> String {
        "Add/remove parentheses".to_string()
    }
}

/// ## What it does
/// Checks for `@pytest.mark.usefixtures()` decorators that aren't passed any
/// arguments.
///
/// ## Why is this bad?
/// A `@pytest.mark.usefixtures()` decorator that isn't passed any arguments is
/// useless and should be removed.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.mark.usefixtures()
/// def test_something():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def test_something():
///     ...
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.mark.usefixtures`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-mark-usefixtures)

#[violation]
pub struct PytestUseFixturesWithoutParameters;

impl AlwaysAutofixableViolation for PytestUseFixturesWithoutParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Useless `pytest.mark.usefixtures` without parameters")
    }

    fn autofix_title(&self) -> String {
        "Remove `usefixtures` decorator or pass parameters".to_string()
    }
}

fn pytest_mark_parentheses(
    checker: &mut Checker,
    decorator: &Decorator,
    call_path: &CallPath,
    fix: Fix,
    preferred: &str,
    actual: &str,
) {
    let mut diagnostic = Diagnostic::new(
        PytestIncorrectMarkParenthesesStyle {
            mark_name: (*call_path.last().unwrap()).to_string(),
            expected_parens: preferred.to_string(),
            actual_parens: actual.to_string(),
        },
        decorator.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(fix);
    }
    checker.diagnostics.push(diagnostic);
}

fn check_mark_parentheses(checker: &mut Checker, decorator: &Decorator, call_path: &CallPath) {
    match &decorator.expression {
        Expr::Call(ast::ExprCall {
            func,
            arguments:
                Arguments {
                    args,
                    keywords,
                    range: _,
                },
            range: _,
        }) => {
            if !checker.settings.flake8_pytest_style.mark_parentheses
                && args.is_empty()
                && keywords.is_empty()
            {
                let fix = Fix::automatic(Edit::deletion(func.end(), decorator.end()));
                pytest_mark_parentheses(checker, decorator, call_path, fix, "", "()");
            }
        }
        _ => {
            if checker.settings.flake8_pytest_style.mark_parentheses {
                let fix = Fix::automatic(Edit::insertion("()".to_string(), decorator.end()));
                pytest_mark_parentheses(checker, decorator, call_path, fix, "()", "");
            }
        }
    }
}

fn check_useless_usefixtures(checker: &mut Checker, decorator: &Decorator, call_path: &CallPath) {
    if *call_path.last().unwrap() != "usefixtures" {
        return;
    }

    let mut has_parameters = false;

    if let Expr::Call(ast::ExprCall {
        arguments: Arguments { args, keywords, .. },
        ..
    }) = &decorator.expression
    {
        if !args.is_empty() || !keywords.is_empty() {
            has_parameters = true;
        }
    }

    if !has_parameters {
        let mut diagnostic = Diagnostic::new(PytestUseFixturesWithoutParameters, decorator.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::suggested(Edit::range_deletion(decorator.range())));
        }
        checker.diagnostics.push(diagnostic);
    }
}

pub(crate) fn marks(checker: &mut Checker, decorators: &[Decorator]) {
    let enforce_parentheses = checker.enabled(Rule::PytestIncorrectMarkParenthesesStyle);
    let enforce_useless_usefixtures = checker.enabled(Rule::PytestUseFixturesWithoutParameters);

    for (decorator, call_path) in get_mark_decorators(decorators) {
        if enforce_parentheses {
            check_mark_parentheses(checker, decorator, &call_path);
        }
        if enforce_useless_usefixtures {
            check_useless_usefixtures(checker, decorator, &call_path);
        }
    }
}
