use rustpython_ast::{Expr, ExprKind, Location};

use super::helpers::{get_mark_decorators, get_mark_name};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};

fn pytest_mark_parentheses(
    checker: &mut Checker,
    decorator: &Expr,
    fix: Fix,
    preferred: &str,
    actual: &str,
) {
    let mut check = Check::new(
        CheckKind::IncorrectMarkParenthesesStyle(
            get_mark_name(decorator).to_string(),
            preferred.to_string(),
            actual.to_string(),
        ),
        Range::from_located(decorator),
    );
    if checker.patch(check.kind.code()) {
        check.amend(fix);
    }
    checker.add_check(check);
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
                let fix = Fix::replacement(
                    String::new(),
                    func.end_location.unwrap(),
                    decorator.end_location.unwrap(),
                );
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
        let mut check = Check::new(
            CheckKind::UseFixturesWithoutParameters,
            Range::from_located(decorator),
        );
        if checker.patch(check.kind.code()) {
            let at_start = Location::new(decorator.location.row(), decorator.location.column() - 1);
            check.amend(Fix::deletion(at_start, decorator.end_location.unwrap()));
        }
        checker.add_check(check);
    }
}

pub fn marks(checker: &mut Checker, decorators: &[Expr]) {
    let enforce_parentheses = checker.settings.enabled.contains(&CheckCode::PT023);
    let enforce_useless_usefixtures = checker.settings.enabled.contains(&CheckCode::PT026);

    for mark in get_mark_decorators(decorators) {
        if enforce_parentheses {
            check_mark_parentheses(checker, mark);
        }
        if enforce_useless_usefixtures {
            check_useless_usefixtures(checker, mark);
        }
    }
}
