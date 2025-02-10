use ruff_python_ast::{self as ast, Arguments, Decorator, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::helpers::{get_mark_decorators, Parentheses};

/// ## What it does
/// Checks for argument-free `@pytest.mark.<marker>()` decorators with or
/// without parentheses, depending on the [`lint.flake8-pytest-style.mark-parentheses`]
/// setting.
///
/// The rule defaults to removing unnecessary parentheses,
/// to match the documentation of the official pytest projects.
///
/// ## Why is this bad?
/// If a `@pytest.mark.<marker>()` doesn't take any arguments, the parentheses are
/// optional.
///
/// Either removing those unnecessary parentheses _or_ requiring them for all
/// fixtures is fine, but it's best to be consistent.
///
/// ## Example
///
/// ```python
/// import pytest
///
///
/// @pytest.mark.foo
/// def test_something(): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import pytest
///
///
/// @pytest.mark.foo()
/// def test_something(): ...
/// ```
///
/// ## Options
/// - `lint.flake8-pytest-style.mark-parentheses`
///
/// ## References
/// - [`pytest` documentation: Marks](https://docs.pytest.org/en/latest/reference/reference.html#marks)
#[derive(ViolationMetadata)]
pub(crate) struct PytestIncorrectMarkParenthesesStyle {
    mark_name: String,
    expected_parens: Parentheses,
    actual_parens: Parentheses,
}

impl AlwaysFixableViolation for PytestIncorrectMarkParenthesesStyle {
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

    fn fix_title(&self) -> String {
        match &self.expected_parens {
            Parentheses::None => "Remove parentheses".to_string(),
            Parentheses::Empty => "Add parentheses".to_string(),
        }
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
///
/// ```python
/// import pytest
///
///
/// @pytest.mark.usefixtures()
/// def test_something(): ...
/// ```
///
/// Use instead:
///
/// ```python
/// def test_something(): ...
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.mark.usefixtures`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-mark-usefixtures)
#[derive(ViolationMetadata)]
pub(crate) struct PytestUseFixturesWithoutParameters;

impl AlwaysFixableViolation for PytestUseFixturesWithoutParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Useless `pytest.mark.usefixtures` without parameters".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `usefixtures` decorator or pass parameters".to_string()
    }
}

fn pytest_mark_parentheses(
    checker: &Checker,
    decorator: &Decorator,
    marker: &str,
    fix: Fix,
    preferred: Parentheses,
    actual: Parentheses,
) {
    let mut diagnostic = Diagnostic::new(
        PytestIncorrectMarkParenthesesStyle {
            mark_name: marker.to_string(),
            expected_parens: preferred,
            actual_parens: actual,
        },
        decorator.range(),
    );
    diagnostic.set_fix(fix);
    checker.report_diagnostic(diagnostic);
}

fn check_mark_parentheses(checker: &Checker, decorator: &Decorator, marker: &str) {
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
                let fix = Fix::safe_edit(Edit::deletion(func.end(), decorator.end()));
                pytest_mark_parentheses(
                    checker,
                    decorator,
                    marker,
                    fix,
                    Parentheses::None,
                    Parentheses::Empty,
                );
            }
        }
        _ => {
            if checker.settings.flake8_pytest_style.mark_parentheses {
                let fix = Fix::safe_edit(Edit::insertion(
                    Parentheses::Empty.to_string(),
                    decorator.end(),
                ));
                pytest_mark_parentheses(
                    checker,
                    decorator,
                    marker,
                    fix,
                    Parentheses::Empty,
                    Parentheses::None,
                );
            }
        }
    }
}

fn check_useless_usefixtures(checker: &Checker, decorator: &Decorator, marker: &str) {
    if marker != "usefixtures" {
        return;
    }

    match &decorator.expression {
        // @pytest.mark.usefixtures
        Expr::Attribute(..) => {}
        // @pytest.mark.usefixtures(...)
        Expr::Call(ast::ExprCall {
            arguments: Arguments { args, keywords, .. },
            ..
        }) => {
            if !args.is_empty() || !keywords.is_empty() {
                return;
            }
        }
        _ => return,
    }

    let mut diagnostic = Diagnostic::new(PytestUseFixturesWithoutParameters, decorator.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(decorator.range())));
    checker.report_diagnostic(diagnostic);
}

pub(crate) fn marks(checker: &Checker, decorators: &[Decorator]) {
    let enforce_parentheses = checker.enabled(Rule::PytestIncorrectMarkParenthesesStyle);
    let enforce_useless_usefixtures = checker.enabled(Rule::PytestUseFixturesWithoutParameters);

    for (decorator, marker) in get_mark_decorators(decorators) {
        if enforce_parentheses {
            check_mark_parentheses(checker, decorator, marker);
        }
        if enforce_useless_usefixtures {
            check_useless_usefixtures(checker, decorator, marker);
        }
    }
}
