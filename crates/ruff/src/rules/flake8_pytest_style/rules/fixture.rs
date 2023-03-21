use anyhow::Result;
use log::error;
use rustpython_parser::ast::{Arguments, Expr, ExprKind, Keyword, Location, Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{collect_arg_names, collect_call_path};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

use crate::autofix::helpers::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::helpers::{
    get_mark_decorators, get_mark_name, is_abstractmethod_decorator, is_pytest_fixture,
    is_pytest_yield_fixture, keyword_is_literal,
};

#[violation]
pub struct PytestFixtureIncorrectParenthesesStyle {
    pub expected_parens: String,
    pub actual_parens: String,
}

impl AlwaysAutofixableViolation for PytestFixtureIncorrectParenthesesStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestFixtureIncorrectParenthesesStyle {
            expected_parens,
            actual_parens,
        } = self;
        format!("Use `@pytest.fixture{expected_parens}` over `@pytest.fixture{actual_parens}`")
    }

    fn autofix_title(&self) -> String {
        "Add/remove parentheses".to_string()
    }
}

#[violation]
pub struct PytestFixturePositionalArgs {
    pub function: String,
}

impl Violation for PytestFixturePositionalArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestFixturePositionalArgs { function } = self;
        format!("Configuration for fixture `{function}` specified via positional args, use kwargs")
    }
}

#[violation]
pub struct PytestExtraneousScopeFunction;

impl AlwaysAutofixableViolation for PytestExtraneousScopeFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`scope='function'` is implied in `@pytest.fixture()`")
    }

    fn autofix_title(&self) -> String {
        "Remove implied `scope` argument".to_string()
    }
}

#[violation]
pub struct PytestMissingFixtureNameUnderscore {
    pub function: String,
}

impl Violation for PytestMissingFixtureNameUnderscore {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestMissingFixtureNameUnderscore { function } = self;
        format!("Fixture `{function}` does not return anything, add leading underscore")
    }
}

#[violation]
pub struct PytestIncorrectFixtureNameUnderscore {
    pub function: String,
}

impl Violation for PytestIncorrectFixtureNameUnderscore {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestIncorrectFixtureNameUnderscore { function } = self;
        format!("Fixture `{function}` returns a value, remove leading underscore")
    }
}

#[violation]
pub struct PytestFixtureParamWithoutValue {
    pub name: String,
}

impl Violation for PytestFixtureParamWithoutValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestFixtureParamWithoutValue { name } = self;
        format!(
            "Fixture `{name}` without value is injected as parameter, use \
             `@pytest.mark.usefixtures` instead"
        )
    }
}

#[violation]
pub struct PytestDeprecatedYieldFixture;

impl Violation for PytestDeprecatedYieldFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`@pytest.yield_fixture` is deprecated, use `@pytest.fixture`")
    }
}

#[violation]
pub struct PytestFixtureFinalizerCallback;

impl Violation for PytestFixtureFinalizerCallback {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `yield` instead of `request.addfinalizer`")
    }
}

#[violation]
pub struct PytestUselessYieldFixture {
    pub name: String,
}

impl AlwaysAutofixableViolation for PytestUselessYieldFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestUselessYieldFixture { name } = self;
        format!("No teardown in fixture `{name}`, use `return` instead of `yield`")
    }

    fn autofix_title(&self) -> String {
        "Replace `yield` with `return`".to_string()
    }
}

#[violation]
pub struct PytestErroneousUseFixturesOnFixture;

impl AlwaysAutofixableViolation for PytestErroneousUseFixturesOnFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pytest.mark.usefixtures` has no effect on fixtures")
    }

    fn autofix_title(&self) -> String {
        "Remove `pytest.mark.usefixtures`".to_string()
    }
}

#[violation]
pub struct PytestUnnecessaryAsyncioMarkOnFixture;

impl AlwaysAutofixableViolation for PytestUnnecessaryAsyncioMarkOnFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pytest.mark.asyncio` is unnecessary for fixtures")
    }

    fn autofix_title(&self) -> String {
        "Remove `pytest.mark.asyncio`".to_string()
    }
}

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
        PytestFixtureIncorrectParenthesesStyle {
            expected_parens: preferred.to_string(),
            actual_parens: actual.to_string(),
        },
        Range::from(decorator),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(fix);
    }
    checker.diagnostics.push(diagnostic);
}

pub fn fix_extraneous_scope_function(
    locator: &Locator,
    stmt_at: Location,
    expr_at: Location,
    expr_end: Location,
    args: &[Expr],
    keywords: &[Keyword],
) -> Result<Fix> {
    remove_argument(locator, stmt_at, expr_at, expr_end, args, keywords, false)
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
                .enabled(Rule::PytestFixtureIncorrectParenthesesStyle)
                && !checker.settings.flake8_pytest_style.fixture_parentheses
                && args.is_empty()
                && keywords.is_empty()
            {
                let fix =
                    Fix::deletion(func.end_location.unwrap(), decorator.end_location.unwrap());
                pytest_fixture_parentheses(checker, decorator, fix, "", "()");
            }

            if checker
                .settings
                .rules
                .enabled(Rule::PytestFixturePositionalArgs)
                && !args.is_empty()
            {
                checker.diagnostics.push(Diagnostic::new(
                    PytestFixturePositionalArgs {
                        function: func_name.to_string(),
                    },
                    Range::from(decorator),
                ));
            }

            if checker
                .settings
                .rules
                .enabled(Rule::PytestExtraneousScopeFunction)
            {
                let scope_keyword = keywords
                    .iter()
                    .find(|kw| kw.node.arg == Some("scope".to_string()));

                if let Some(scope_keyword) = scope_keyword {
                    if keyword_is_literal(scope_keyword, "function") {
                        let mut diagnostic = Diagnostic::new(
                            PytestExtraneousScopeFunction,
                            Range::from(scope_keyword),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            match fix_extraneous_scope_function(
                                checker.locator,
                                decorator.location,
                                diagnostic.location,
                                diagnostic.end_location,
                                args,
                                keywords,
                            ) {
                                Ok(fix) => {
                                    diagnostic.amend(fix);
                                }
                                Err(e) => error!("Failed to generate fix: {e}"),
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        _ => {
            if checker
                .settings
                .rules
                .enabled(Rule::PytestFixtureIncorrectParenthesesStyle)
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
        .enabled(Rule::PytestIncorrectFixtureNameUnderscore)
        && visitor.has_return_with_value
        && func_name.starts_with('_')
    {
        checker.diagnostics.push(Diagnostic::new(
            PytestIncorrectFixtureNameUnderscore {
                function: func_name.to_string(),
            },
            Range::from(func),
        ));
    } else if checker
        .settings
        .rules
        .enabled(Rule::PytestMissingFixtureNameUnderscore)
        && !visitor.has_return_with_value
        && !visitor.has_yield_from
        && !func_name.starts_with('_')
    {
        checker.diagnostics.push(Diagnostic::new(
            PytestMissingFixtureNameUnderscore {
                function: func_name.to_string(),
            },
            Range::from(func),
        ));
    }

    if checker
        .settings
        .rules
        .enabled(Rule::PytestUselessYieldFixture)
    {
        if let Some(stmt) = body.last() {
            if let StmtKind::Expr { value, .. } = &stmt.node {
                if let ExprKind::Yield { .. } = value.node {
                    if visitor.yield_statements.len() == 1 {
                        let mut diagnostic = Diagnostic::new(
                            PytestUselessYieldFixture {
                                name: func_name.to_string(),
                            },
                            Range::from(stmt),
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
                PytestFixtureParamWithoutValue {
                    name: name.to_string(),
                },
                Range::from(arg),
            ));
        }
    });
}

/// PT020
fn check_fixture_decorator_name(checker: &mut Checker, decorator: &Expr) {
    if is_pytest_yield_fixture(decorator, checker) {
        checker.diagnostics.push(Diagnostic::new(
            PytestDeprecatedYieldFixture,
            Range::from(decorator),
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
            PytestFixtureFinalizerCallback,
            Range::from(addfinalizer),
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
            .enabled(Rule::PytestUnnecessaryAsyncioMarkOnFixture)
        {
            if name == "asyncio" {
                let mut diagnostic =
                    Diagnostic::new(PytestUnnecessaryAsyncioMarkOnFixture, Range::from(mark));
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
            .enabled(Rule::PytestErroneousUseFixturesOnFixture)
        {
            if name == "usefixtures" {
                let mut diagnostic =
                    Diagnostic::new(PytestErroneousUseFixturesOnFixture, Range::from(mark));
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
            .enabled(Rule::PytestFixtureIncorrectParenthesesStyle)
            || checker
                .settings
                .rules
                .enabled(Rule::PytestFixturePositionalArgs)
            || checker
                .settings
                .rules
                .enabled(Rule::PytestExtraneousScopeFunction)
        {
            check_fixture_decorator(checker, func_name, decorator);
        }

        if checker
            .settings
            .rules
            .enabled(Rule::PytestDeprecatedYieldFixture)
            && checker.settings.flake8_pytest_style.fixture_parentheses
        {
            check_fixture_decorator_name(checker, decorator);
        }

        if (checker
            .settings
            .rules
            .enabled(Rule::PytestMissingFixtureNameUnderscore)
            || checker
                .settings
                .rules
                .enabled(Rule::PytestIncorrectFixtureNameUnderscore)
            || checker
                .settings
                .rules
                .enabled(Rule::PytestUselessYieldFixture))
            && !has_abstractmethod_decorator(decorators, checker)
        {
            check_fixture_returns(checker, func, func_name, body);
        }

        if checker
            .settings
            .rules
            .enabled(Rule::PytestFixtureFinalizerCallback)
        {
            check_fixture_addfinalizer(checker, args, body);
        }

        if checker
            .settings
            .rules
            .enabled(Rule::PytestUnnecessaryAsyncioMarkOnFixture)
            || checker
                .settings
                .rules
                .enabled(Rule::PytestErroneousUseFixturesOnFixture)
        {
            check_fixture_marks(checker, decorators);
        }
    }

    if checker
        .settings
        .rules
        .enabled(Rule::PytestFixtureParamWithoutValue)
        && func_name.starts_with("test_")
    {
        check_test_function_args(checker, args);
    }
}
