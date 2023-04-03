use rustpython_parser::ast::{Expr, ExprKind, Location};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::helpers::get_mark_decorators;

#[violation]
pub struct PytestIncorrectMarkParenthesesStyle {
    pub mark_name: String,
    pub expected_parens: String,
    pub actual_parens: String,
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
    decorator: &Expr,
    call_path: &CallPath,
    fix: Edit,
    preferred: &str,
    actual: &str,
) {
    let mut diagnostic = Diagnostic::new(
        PytestIncorrectMarkParenthesesStyle {
            mark_name: (*call_path.last().unwrap()).to_string(),
            expected_parens: preferred.to_string(),
            actual_parens: actual.to_string(),
        },
        Range::from(decorator),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(fix);
    }
    checker.diagnostics.push(diagnostic);
}

fn check_mark_parentheses(checker: &mut Checker, decorator: &Expr, call_path: &CallPath) {
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
                    Edit::deletion(func.end_location.unwrap(), decorator.end_location.unwrap());
                pytest_mark_parentheses(checker, decorator, call_path, fix, "", "()");
            }
        }
        _ => {
            if checker.settings.flake8_pytest_style.mark_parentheses {
                let fix = Edit::insertion("()".to_string(), decorator.end_location.unwrap());
                pytest_mark_parentheses(checker, decorator, call_path, fix, "()", "");
            }
        }
    }
}

fn check_useless_usefixtures(checker: &mut Checker, decorator: &Expr, call_path: &CallPath) {
    if *call_path.last().unwrap() != "usefixtures" {
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
            Diagnostic::new(PytestUseFixturesWithoutParameters, Range::from(decorator));
        if checker.patch(diagnostic.kind.rule()) {
            let at_start = Location::new(decorator.location.row(), decorator.location.column() - 1);
            diagnostic.set_fix(Edit::deletion(at_start, decorator.end_location.unwrap()));
        }
        checker.diagnostics.push(diagnostic);
    }
}

pub fn marks(checker: &mut Checker, decorators: &[Expr]) {
    let enforce_parentheses = checker
        .settings
        .rules
        .enabled(Rule::PytestIncorrectMarkParenthesesStyle);
    let enforce_useless_usefixtures = checker
        .settings
        .rules
        .enabled(Rule::PytestUseFixturesWithoutParameters);

    for (expr, call_path) in get_mark_decorators(decorators) {
        if enforce_parentheses {
            check_mark_parentheses(checker, expr, &call_path);
        }
        if enforce_useless_usefixtures {
            check_useless_usefixtures(checker, expr, &call_path);
        }
    }
}
