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
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

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

fn get_fixture_decorator<'a>(xxxxxxxx: &xxxxxxxx, decorators: &'a [Expr]) -> Option<&'a Expr> {
    decorators.iter().find(|decorator| {
        is_pytest_fixture(decorator, xxxxxxxx) || is_pytest_yield_fixture(decorator, xxxxxxxx)
    })
}

fn has_abstractmethod_decorator(decorators: &[Expr], xxxxxxxx: &xxxxxxxx) -> bool {
    decorators
        .iter()
        .any(|decorator| is_abstractmethod_decorator(decorator, xxxxxxxx))
}

fn pytest_fixture_parentheses(
    xxxxxxxx: &mut xxxxxxxx,
    decorator: &Expr,
    fix: Fix,
    preferred: &str,
    actual: &str,
) {
    let mut check = Diagnostic::new(
        violations::IncorrectFixtureParenthesesStyle(preferred.to_string(), actual.to_string()),
        Range::from_located(decorator),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(fix);
    }
    xxxxxxxx.diagnostics.push(check);
}

/// PT001, PT002, PT003
fn check_fixture_decorator(xxxxxxxx: &mut xxxxxxxx, func_name: &str, decorator: &Expr) {
    match &decorator.node {
        ExprKind::Call {
            func,
            args,
            keywords,
            ..
        } => {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::PT001)
                && !xxxxxxxx.settings.flake8_pytest_style.fixture_parentheses
                && args.is_empty()
                && keywords.is_empty()
            {
                let fix = Fix::replacement(
                    String::new(),
                    func.end_location.unwrap(),
                    decorator.end_location.unwrap(),
                );
                pytest_fixture_parentheses(xxxxxxxx, decorator, fix, "", "()");
            }

            if xxxxxxxx.settings.enabled.contains(&RuleCode::PT002) && !args.is_empty() {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::FixturePositionalArgs(func_name.to_string()),
                    Range::from_located(decorator),
                ));
            }

            if xxxxxxxx.settings.enabled.contains(&RuleCode::PT003) {
                let scope_keyword = keywords
                    .iter()
                    .find(|kw| kw.node.arg == Some("scope".to_string()));

                if let Some(scope_keyword) = scope_keyword {
                    if keyword_is_literal(scope_keyword, "function") {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::ExtraneousScopeFunction,
                            Range::from_located(scope_keyword),
                        ));
                    }
                }
            }
        }
        _ => {
            if xxxxxxxx.settings.enabled.contains(&RuleCode::PT001)
                && xxxxxxxx.settings.flake8_pytest_style.fixture_parentheses
            {
                let fix = Fix::insertion("()".to_string(), decorator.end_location.unwrap());
                pytest_fixture_parentheses(xxxxxxxx, decorator, fix, "()", "");
            }
        }
    }
}

/// PT004, PT005, PT022
fn check_fixture_returns(xxxxxxxx: &mut xxxxxxxx, func: &Stmt, func_name: &str, body: &[Stmt]) {
    let mut visitor = SkipFunctionsVisitor::default();

    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::PT005)
        && visitor.has_return_with_value
        && func_name.starts_with('_')
    {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::IncorrectFixtureNameUnderscore(func_name.to_string()),
            Range::from_located(func),
        ));
    } else if xxxxxxxx.settings.enabled.contains(&RuleCode::PT004)
        && !visitor.has_return_with_value
        && !visitor.has_yield_from
        && !func_name.starts_with('_')
    {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::MissingFixtureNameUnderscore(func_name.to_string()),
            Range::from_located(func),
        ));
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::PT022) {
        if let Some(stmt) = body.last() {
            if let StmtKind::Expr { value, .. } = &stmt.node {
                if let ExprKind::Yield { .. } = value.node {
                    if visitor.yield_statements.len() == 1 {
                        let mut check = Diagnostic::new(
                            violations::UselessYieldFixture(func_name.to_string()),
                            Range::from_located(stmt),
                        );
                        if xxxxxxxx.patch(check.kind.code()) {
                            check.amend(Fix::replacement(
                                "return".to_string(),
                                stmt.location,
                                Location::new(
                                    stmt.location.row(),
                                    stmt.location.column() + "yield".len(),
                                ),
                            ));
                        }
                        xxxxxxxx.diagnostics.push(check);
                    }
                }
            }
        }
    }
}

/// PT019
fn check_test_function_args(xxxxxxxx: &mut xxxxxxxx, args: &Arguments) {
    args.args.iter().chain(&args.kwonlyargs).for_each(|arg| {
        let name = arg.node.arg.to_string();
        if name.starts_with('_') {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::FixtureParamWithoutValue(name),
                Range::from_located(arg),
            ));
        }
    });
}

/// PT020
fn check_fixture_decorator_name(xxxxxxxx: &mut xxxxxxxx, decorator: &Expr) {
    if is_pytest_yield_fixture(decorator, xxxxxxxx) {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::DeprecatedYieldFixture,
            Range::from_located(decorator),
        ));
    }
}

/// PT021
fn check_fixture_addfinalizer(xxxxxxxx: &mut xxxxxxxx, args: &Arguments, body: &[Stmt]) {
    if !collect_arg_names(args).contains(&"request") {
        return;
    }

    let mut visitor = SkipFunctionsVisitor::default();

    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if let Some(addfinalizer) = visitor.addfinalizer_call {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::FixtureFinalizerCallback,
            Range::from_located(addfinalizer),
        ));
    }
}

/// PT024, PT025
fn check_fixture_marks(xxxxxxxx: &mut xxxxxxxx, decorators: &[Expr]) {
    for mark in get_mark_decorators(decorators) {
        let name = get_mark_name(mark);

        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT024) {
            if name == "asyncio" {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::UnnecessaryAsyncioMarkOnFixture,
                    Range::from_located(mark),
                ));
            }
        }

        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT025) {
            if name == "usefixtures" {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::ErroneousUseFixturesOnFixture,
                    Range::from_located(mark),
                ));
            }
        }
    }
}

pub fn fixture(
    xxxxxxxx: &mut xxxxxxxx,
    func: &Stmt,
    func_name: &str,
    args: &Arguments,
    decorators: &[Expr],
    body: &[Stmt],
) {
    let decorator = get_fixture_decorator(xxxxxxxx, decorators);
    if let Some(decorator) = decorator {
        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT001)
            || xxxxxxxx.settings.enabled.contains(&RuleCode::PT002)
            || xxxxxxxx.settings.enabled.contains(&RuleCode::PT003)
        {
            check_fixture_decorator(xxxxxxxx, func_name, decorator);
        }

        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT020)
            && xxxxxxxx.settings.flake8_pytest_style.fixture_parentheses
        {
            check_fixture_decorator_name(xxxxxxxx, decorator);
        }

        if (xxxxxxxx.settings.enabled.contains(&RuleCode::PT004)
            || xxxxxxxx.settings.enabled.contains(&RuleCode::PT005)
            || xxxxxxxx.settings.enabled.contains(&RuleCode::PT022))
            && !has_abstractmethod_decorator(decorators, xxxxxxxx)
        {
            check_fixture_returns(xxxxxxxx, func, func_name, body);
        }

        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT021) {
            check_fixture_addfinalizer(xxxxxxxx, args, body);
        }

        if xxxxxxxx.settings.enabled.contains(&RuleCode::PT024)
            || xxxxxxxx.settings.enabled.contains(&RuleCode::PT025)
        {
            check_fixture_marks(xxxxxxxx, decorators);
        }
    }

    if xxxxxxxx.settings.enabled.contains(&RuleCode::PT019) && func_name.starts_with("test_") {
        check_test_function_args(xxxxxxxx, args);
    }
}
