use rustpython_ast::{Arguments, Expr, ExprKind, Location, Stmt, StmtKind};

use super::helpers::{
    get_mark_decorators, get_mark_name, is_abstractmethod_decorator, is_pytest_fixture,
    is_pytest_yield_fixture, keyword_is_literal,
};
use crate::ast::helpers::{collect_arg_names, collect_call_paths};
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};

#[derive(Default)]
/// Visitor that skips functions
struct SkipFunctionsVisitor<'a> {
    has_return_with_value: bool,
    has_yield_from: bool,
    yield_statements: Vec<&'a Expr>,
    addfinalizer_call: Option<&'a Expr>,
}

impl<'a, 'b> Visitor<'b> for SkipFunctionsVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::Return { value, .. } => {
                if value.is_some() {
                    self.has_return_with_value = true;
                }
            }
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {}
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::YieldFrom { .. } => {
                self.has_yield_from = true;
            }
            ExprKind::Yield { value, .. } => {
                self.yield_statements.push(expr);
                if value.is_some() {
                    self.has_return_with_value = true;
                }
            }
            ExprKind::Call { func, .. } => {
                if collect_call_paths(func) == vec!["request", "addfinalizer"] {
                    self.addfinalizer_call = Some(expr);
                };
                visitor::walk_expr(self, expr);
            }
            _ => {}
        }
    }
}

fn get_fixture_decorator<'a>(checker: &Checker, decorators: &'a [Expr]) -> Option<&'a Expr> {
    decorators.iter().find(|decorator| {
        is_pytest_fixture(decorator, checker) || is_pytest_yield_fixture(decorator, checker)
    })
}

fn has_abstractmethod_decorator(decorators: &[Expr], checker: &Checker) -> bool {
    decorators
        .iter()
        .any(|decorator| is_abstractmethod_decorator(decorator, checker))
}

fn pytest_fixture_parentheses(
    checker: &mut Checker,
    decorator: &Expr,
    fix: Fix,
    preferred: &str,
    actual: &str,
) {
    let mut check = Check::new(
        CheckKind::IncorrectFixtureParenthesesStyle(preferred.to_string(), actual.to_string()),
        Range::from_located(decorator),
    );
    if checker.patch(check.kind.code()) {
        check.amend(fix);
    }
    checker.add_check(check);
}

/// PT001, PT002, PT003
fn check_fixture_decorator(checker: &mut Checker, func_name: &str, decorator: &Expr) {
    match &decorator.node {
        ExprKind::Call {
            func,
            args,
            keywords,
            ..
        } => {
            if checker.settings.enabled.contains(&CheckCode::PT001)
                && !checker.settings.flake8_pytest_style.fixture_parentheses
                && args.is_empty()
                && keywords.is_empty()
            {
                let fix = Fix::replacement(
                    String::new(),
                    func.end_location.unwrap(),
                    decorator.end_location.unwrap(),
                );
                pytest_fixture_parentheses(checker, decorator, fix, "", "()");
            }

            if checker.settings.enabled.contains(&CheckCode::PT002) && !args.is_empty() {
                checker.add_check(Check::new(
                    CheckKind::FixturePositionalArgs(func_name.to_string()),
                    Range::from_located(decorator),
                ));
            }

            if checker.settings.enabled.contains(&CheckCode::PT003) {
                let scope_keyword = keywords
                    .iter()
                    .find(|kw| kw.node.arg == Some("scope".to_string()));

                if let Some(scope_keyword) = scope_keyword {
                    if keyword_is_literal(scope_keyword, "function") {
                        checker.add_check(Check::new(
                            CheckKind::ExtraneousScopeFunction,
                            Range::from_located(scope_keyword),
                        ));
                    }
                }
            }
        }
        _ => {
            if checker.settings.enabled.contains(&CheckCode::PT001)
                && checker.settings.flake8_pytest_style.fixture_parentheses
            {
                let fix = Fix::insertion("()".to_string(), decorator.end_location.unwrap());
                pytest_fixture_parentheses(checker, decorator, fix, "()", "");
            }
        }
    }
}

/// PT004, PT005, PT022
fn check_fixture_returns(checker: &mut Checker, func: &Stmt, func_name: &str, body: &[Stmt]) {
    let mut visitor = SkipFunctionsVisitor::default();

    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if checker.settings.enabled.contains(&CheckCode::PT005)
        && visitor.has_return_with_value
        && func_name.starts_with('_')
    {
        checker.add_check(Check::new(
            CheckKind::IncorrectFixtureNameUnderscore(func_name.to_string()),
            Range::from_located(func),
        ));
    } else if checker.settings.enabled.contains(&CheckCode::PT004)
        && !visitor.has_return_with_value
        && !visitor.has_yield_from
        && !func_name.starts_with('_')
    {
        checker.add_check(Check::new(
            CheckKind::MissingFixtureNameUnderscore(func_name.to_string()),
            Range::from_located(func),
        ));
    }

    if checker.settings.enabled.contains(&CheckCode::PT022) {
        if let Some(stmt) = body.last() {
            if let StmtKind::Expr { value, .. } = &stmt.node {
                if let ExprKind::Yield { .. } = value.node {
                    if visitor.yield_statements.len() == 1 {
                        let mut check = Check::new(
                            CheckKind::UselessYieldFixture(func_name.to_string()),
                            Range::from_located(stmt),
                        );
                        if checker.patch(check.kind.code()) {
                            check.amend(Fix::replacement(
                                "return".to_string(),
                                stmt.location,
                                Location::new(
                                    stmt.location.row(),
                                    stmt.location.column() + "yield".len(),
                                ),
                            ));
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
    }
}

/// PT019
fn check_test_function_args(checker: &mut Checker, args: &Arguments) {
    args.args.iter().chain(&args.kwonlyargs).for_each(|arg| {
        let name = arg.node.arg.to_string();
        if name.starts_with('_') {
            checker.add_check(Check::new(
                CheckKind::FixtureParamWithoutValue(name),
                Range::from_located(arg),
            ));
        }
    });
}

/// PT020
fn check_fixture_decorator_name(checker: &mut Checker, decorator: &Expr) {
    if is_pytest_yield_fixture(decorator, checker) {
        checker.add_check(Check::new(
            CheckKind::DeprecatedYieldFixture,
            Range::from_located(decorator),
        ));
    }
}

/// PT021
fn check_fixture_addfinalizer(checker: &mut Checker, args: &Arguments, body: &[Stmt]) {
    if !collect_arg_names(args).contains(&"request") {
        return;
    }

    let mut visitor = SkipFunctionsVisitor::default();

    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if let Some(addfinalizer) = visitor.addfinalizer_call {
        checker.add_check(Check::new(
            CheckKind::FixtureFinalizerCallback,
            Range::from_located(addfinalizer),
        ));
    }
}

/// PT024, PT025
fn check_fixture_marks(checker: &mut Checker, decorators: &[Expr]) {
    for mark in get_mark_decorators(decorators) {
        let name = get_mark_name(mark);

        if checker.settings.enabled.contains(&CheckCode::PT024) {
            if name == "asyncio" {
                checker.add_check(Check::new(
                    CheckKind::UnnecessaryAsyncioMarkOnFixture,
                    Range::from_located(mark),
                ));
            }
        }

        if checker.settings.enabled.contains(&CheckCode::PT025) {
            if name == "usefixtures" {
                checker.add_check(Check::new(
                    CheckKind::ErroneousUseFixturesOnFixture,
                    Range::from_located(mark),
                ));
            }
        }
    }
}

pub fn fixture(
    checker: &mut Checker,
    func: &Stmt,
    func_name: &str,
    args: &Arguments,
    decorators: &[Expr],
    body: &[Stmt],
) {
    let decorator = get_fixture_decorator(checker, decorators);
    if let Some(decorator) = decorator {
        if checker.settings.enabled.contains(&CheckCode::PT001)
            || checker.settings.enabled.contains(&CheckCode::PT002)
            || checker.settings.enabled.contains(&CheckCode::PT003)
        {
            check_fixture_decorator(checker, func_name, decorator);
        }

        if checker.settings.enabled.contains(&CheckCode::PT020)
            && checker.settings.flake8_pytest_style.fixture_parentheses
        {
            check_fixture_decorator_name(checker, decorator);
        }

        if (checker.settings.enabled.contains(&CheckCode::PT004)
            || checker.settings.enabled.contains(&CheckCode::PT005)
            || checker.settings.enabled.contains(&CheckCode::PT022))
            && !has_abstractmethod_decorator(decorators, checker)
        {
            check_fixture_returns(checker, func, func_name, body);
        }

        if checker.settings.enabled.contains(&CheckCode::PT021) {
            check_fixture_addfinalizer(checker, args, body);
        }

        if checker.settings.enabled.contains(&CheckCode::PT024)
            || checker.settings.enabled.contains(&CheckCode::PT025)
        {
            check_fixture_marks(checker, decorators);
        }
    }

    if checker.settings.enabled.contains(&CheckCode::PT019) && func_name.starts_with("test_") {
        check_test_function_args(checker, args);
    }
}
