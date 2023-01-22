use rustpython_ast::{Arguments, Expr, ExprKind, Location, Stmt, StmtKind};

use super::helpers::{
    get_mark_decorators, get_mark_name, is_abstractmethod_decorator, is_pytest_fixture,
    is_pytest_yield_fixture, keyword_is_literal,
};
use crate::ast::helpers::{collect_arg_names, collect_call_path};
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

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
                if collect_call_path(func).as_slice() == ["request", "addfinalizer"] {
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
    let mut diagnostic = Diagnostic::new(
        violations::IncorrectFixtureParenthesesStyle(preferred.to_string(), actual.to_string()),
        Range::from_located(decorator),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(fix);
    }
    checker.diagnostics.push(diagnostic);
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
            if checker
                .settings
                .rules
                .enabled(&Rule::IncorrectFixtureParenthesesStyle)
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

            if checker.settings.rules.enabled(&Rule::FixturePositionalArgs) && !args.is_empty() {
                checker.diagnostics.push(Diagnostic::new(
                    violations::FixturePositionalArgs(func_name.to_string()),
                    Range::from_located(decorator),
                ));
            }

            if checker
                .settings
                .rules
                .enabled(&Rule::ExtraneousScopeFunction)
            {
                let scope_keyword = keywords
                    .iter()
                    .find(|kw| kw.node.arg == Some("scope".to_string()));

                if let Some(scope_keyword) = scope_keyword {
                    if keyword_is_literal(scope_keyword, "function") {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::ExtraneousScopeFunction,
                            Range::from_located(scope_keyword),
                        ));
                    }
                }
            }
        }
        _ => {
            if checker
                .settings
                .rules
                .enabled(&Rule::IncorrectFixtureParenthesesStyle)
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

    if checker
        .settings
        .rules
        .enabled(&Rule::IncorrectFixtureNameUnderscore)
        && visitor.has_return_with_value
        && func_name.starts_with('_')
    {
        checker.diagnostics.push(Diagnostic::new(
            violations::IncorrectFixtureNameUnderscore(func_name.to_string()),
            Range::from_located(func),
        ));
    } else if checker
        .settings
        .rules
        .enabled(&Rule::MissingFixtureNameUnderscore)
        && !visitor.has_return_with_value
        && !visitor.has_yield_from
        && !func_name.starts_with('_')
    {
        checker.diagnostics.push(Diagnostic::new(
            violations::MissingFixtureNameUnderscore(func_name.to_string()),
            Range::from_located(func),
        ));
    }

    if checker.settings.rules.enabled(&Rule::UselessYieldFixture) {
        if let Some(stmt) = body.last() {
            if let StmtKind::Expr { value, .. } = &stmt.node {
                if let ExprKind::Yield { .. } = value.node {
                    if visitor.yield_statements.len() == 1 {
                        let mut diagnostic = Diagnostic::new(
                            violations::UselessYieldFixture(func_name.to_string()),
                            Range::from_located(stmt),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.amend(Fix::replacement(
                                "return".to_string(),
                                stmt.location,
                                Location::new(
                                    stmt.location.row(),
                                    stmt.location.column() + "yield".len(),
                                ),
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }
}

/// PT019
fn check_test_function_args(checker: &mut Checker, args: &Arguments) {
    args.args.iter().chain(&args.kwonlyargs).for_each(|arg| {
        let name = &arg.node.arg;
        if name.starts_with('_') {
            checker.diagnostics.push(Diagnostic::new(
                violations::FixtureParamWithoutValue(name.to_string()),
                Range::from_located(arg),
            ));
        }
    });
}

/// PT020
fn check_fixture_decorator_name(checker: &mut Checker, decorator: &Expr) {
    if is_pytest_yield_fixture(decorator, checker) {
        checker.diagnostics.push(Diagnostic::new(
            violations::DeprecatedYieldFixture,
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
        checker.diagnostics.push(Diagnostic::new(
            violations::FixtureFinalizerCallback,
            Range::from_located(addfinalizer),
        ));
    }
}

/// PT024, PT025
fn check_fixture_marks(checker: &mut Checker, decorators: &[Expr]) {
    for mark in get_mark_decorators(decorators) {
        let name = get_mark_name(mark);

        if checker
            .settings
            .rules
            .enabled(&Rule::UnnecessaryAsyncioMarkOnFixture)
        {
            if name == "asyncio" {
                let mut diagnostic = Diagnostic::new(
                    violations::UnnecessaryAsyncioMarkOnFixture,
                    Range::from_located(mark),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    let start = Location::new(mark.location.row(), 0);
                    let end = Location::new(mark.end_location.unwrap().row() + 1, 0);
                    diagnostic.amend(Fix::deletion(start, end));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        if checker
            .settings
            .rules
            .enabled(&Rule::ErroneousUseFixturesOnFixture)
        {
            if name == "usefixtures" {
                let mut diagnostic = Diagnostic::new(
                    violations::ErroneousUseFixturesOnFixture,
                    Range::from_located(mark),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    let start = Location::new(mark.location.row(), 0);
                    let end = Location::new(mark.end_location.unwrap().row() + 1, 0);
                    diagnostic.amend(Fix::deletion(start, end));
                }
                checker.diagnostics.push(diagnostic);
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
        if checker
            .settings
            .rules
            .enabled(&Rule::IncorrectFixtureParenthesesStyle)
            || checker.settings.rules.enabled(&Rule::FixturePositionalArgs)
            || checker
                .settings
                .rules
                .enabled(&Rule::ExtraneousScopeFunction)
        {
            check_fixture_decorator(checker, func_name, decorator);
        }

        if checker
            .settings
            .rules
            .enabled(&Rule::DeprecatedYieldFixture)
            && checker.settings.flake8_pytest_style.fixture_parentheses
        {
            check_fixture_decorator_name(checker, decorator);
        }

        if (checker
            .settings
            .rules
            .enabled(&Rule::MissingFixtureNameUnderscore)
            || checker
                .settings
                .rules
                .enabled(&Rule::IncorrectFixtureNameUnderscore)
            || checker.settings.rules.enabled(&Rule::UselessYieldFixture))
            && !has_abstractmethod_decorator(decorators, checker)
        {
            check_fixture_returns(checker, func, func_name, body);
        }

        if checker
            .settings
            .rules
            .enabled(&Rule::FixtureFinalizerCallback)
        {
            check_fixture_addfinalizer(checker, args, body);
        }

        if checker
            .settings
            .rules
            .enabled(&Rule::UnnecessaryAsyncioMarkOnFixture)
            || checker
                .settings
                .rules
                .enabled(&Rule::ErroneousUseFixturesOnFixture)
        {
            check_fixture_marks(checker, decorators);
        }
    }

    if checker
        .settings
        .rules
        .enabled(&Rule::FixtureParamWithoutValue)
        && func_name.starts_with("test_")
    {
        check_test_function_args(checker, args);
    }
}
