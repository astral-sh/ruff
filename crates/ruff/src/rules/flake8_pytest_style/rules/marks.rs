use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Location};

use super::helpers::{get_mark_decorators, get_mark_name};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct IncorrectMarkParenthesesStyle {
        pub mark_name: String,
        pub expected_parens: String,
        pub actual_parens: String,
    }
);
impl AlwaysAutofixableViolation for IncorrectMarkParenthesesStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectMarkParenthesesStyle {
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

define_violation!(
    pub struct UseFixturesWithoutParameters;
);
impl AlwaysAutofixableViolation for UseFixturesWithoutParameters {
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
    decorator: &Expr,
    fix: Fix,
    preferred: &str,
    actual: &str,
) {
    let mut diagnostic = Diagnostic::new(
        IncorrectMarkParenthesesStyle {
            mark_name: get_mark_name(decorator).to_string(),
            expected_parens: preferred.to_string(),
            actual_parens: actual.to_string(),
        },
        Range::from_located(decorator),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(fix);
    }
    checker.diagnostics.push(diagnostic);
}

fn check_mark_parentheses(checker: &mut Checker, decorator: &Expr) {
    match &decorator.node {
        ExprKind::Call {
            func,
            args,
            keywords,
            ..
        } => {
            if !checker.settings.flake8_pytest_style.mark_parentheses
                && args.is_empty()
                && keywords.is_empty()
            {
                let fix =
                    Fix::deletion(func.end_location.unwrap(), decorator.end_location.unwrap());
                pytest_mark_parentheses(checker, decorator, fix, "", "()");
            }
        }
        _ => {
            if checker.settings.flake8_pytest_style.mark_parentheses {
                let fix = Fix::insertion("()".to_string(), decorator.end_location.unwrap());
                pytest_mark_parentheses(checker, decorator, fix, "()", "");
            }
        }
    }
}

fn check_useless_usefixtures(checker: &mut Checker, decorator: &Expr) {
    if get_mark_name(decorator) != "usefixtures" {
        return;
    }

    let mut has_parameters = false;

    if let ExprKind::Call { args, keywords, .. } = &decorator.node {
        if !args.is_empty() || !keywords.is_empty() {
            has_parameters = true;
        }
    }

    if !has_parameters {
        let mut diagnostic =
            Diagnostic::new(UseFixturesWithoutParameters, Range::from_located(decorator));
        if checker.patch(diagnostic.kind.rule()) {
            let at_start = Location::new(decorator.location.row(), decorator.location.column() - 1);
            diagnostic.amend(Fix::deletion(at_start, decorator.end_location.unwrap()));
        }
        checker.diagnostics.push(diagnostic);
    }
}

pub fn marks(checker: &mut Checker, decorators: &[Expr]) {
    let enforce_parentheses = checker
        .settings
        .rules
        .enabled(&Rule::IncorrectMarkParenthesesStyle);
    let enforce_useless_usefixtures = checker
        .settings
        .rules
        .enabled(&Rule::UseFixturesWithoutParameters);

    for mark in get_mark_decorators(decorators) {
        if enforce_parentheses {
            check_mark_parentheses(checker, mark);
        }
        if enforce_useless_usefixtures {
            check_useless_usefixtures(checker, mark);
        }
    }
}
