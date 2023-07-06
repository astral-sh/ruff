use std::fmt;

use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Decorator;
use rustpython_parser::ast::{self, ArgWithDefault, Arguments, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::helpers::includes_arg_name;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::visibility::is_abstract;
use ruff_python_semantic::SemanticModel;

use crate::autofix::edits::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::helpers::{
    get_mark_decorators, is_pytest_fixture, is_pytest_yield_fixture, keyword_is_literal,
};

#[violation]
pub struct PytestFixtureIncorrectParenthesesStyle {
    expected: Parentheses,
    actual: Parentheses,
}

impl AlwaysAutofixableViolation for PytestFixtureIncorrectParenthesesStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestFixtureIncorrectParenthesesStyle { expected, actual } = self;
        format!("Use `@pytest.fixture{expected}` over `@pytest.fixture{actual}`")
    }

    fn autofix_title(&self) -> String {
        let PytestFixtureIncorrectParenthesesStyle { expected, .. } = self;
        match expected {
            Parentheses::None => "Remove parentheses".to_string(),
            Parentheses::Empty => "Add parentheses".to_string(),
        }
    }
}

#[violation]
pub struct PytestFixturePositionalArgs {
    function: String,
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
    function: String,
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
    function: String,
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
    name: String,
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
    name: String,
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

#[derive(Debug, PartialEq, Eq)]
enum Parentheses {
    None,
    Empty,
}

impl fmt::Display for Parentheses {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Parentheses::None => fmt.write_str(""),
            Parentheses::Empty => fmt.write_str("()"),
        }
    }
}

/// Visitor that skips functions
#[derive(Debug, Default)]
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
        match stmt {
            Stmt::Return(ast::StmtReturn { value, range: _ }) => {
                if value.is_some() {
                    self.has_return_with_value = true;
                }
            }
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) => {}
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        match expr {
            Expr::YieldFrom(_) => {
                self.has_yield_from = true;
            }
            Expr::Yield(ast::ExprYield { value, range: _ }) => {
                self.yield_statements.push(expr);
                if value.is_some() {
                    self.has_return_with_value = true;
                }
            }
            Expr::Call(ast::ExprCall { func, .. }) => {
                if collect_call_path(func).map_or(false, |call_path| {
                    matches!(call_path.as_slice(), ["request", "addfinalizer"])
                }) {
                    self.addfinalizer_call = Some(expr);
                };
                visitor::walk_expr(self, expr);
            }
            _ => {}
        }
    }
}

fn fixture_decorator<'a>(
    decorators: &'a [Decorator],
    semantic: &SemanticModel,
) -> Option<&'a Decorator> {
    decorators.iter().find(|decorator| {
        is_pytest_fixture(decorator, semantic) || is_pytest_yield_fixture(decorator, semantic)
    })
}

fn pytest_fixture_parentheses(
    checker: &mut Checker,
    decorator: &Decorator,
    fix: Fix,
    expected: Parentheses,
    actual: Parentheses,
) {
    let mut diagnostic = Diagnostic::new(
        PytestFixtureIncorrectParenthesesStyle { expected, actual },
        decorator.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(fix);
    }
    checker.diagnostics.push(diagnostic);
}

/// PT001, PT002, PT003
fn check_fixture_decorator(checker: &mut Checker, func_name: &str, decorator: &Decorator) {
    match &decorator.expression {
        Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
            range: _,
        }) => {
            if checker.enabled(Rule::PytestFixtureIncorrectParenthesesStyle) {
                if !checker.settings.flake8_pytest_style.fixture_parentheses
                    && args.is_empty()
                    && keywords.is_empty()
                {
                    let fix = Fix::automatic(Edit::deletion(func.end(), decorator.end()));
                    pytest_fixture_parentheses(
                        checker,
                        decorator,
                        fix,
                        Parentheses::None,
                        Parentheses::Empty,
                    );
                }
            }

            if checker.enabled(Rule::PytestFixturePositionalArgs) {
                if !args.is_empty() {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestFixturePositionalArgs {
                            function: func_name.to_string(),
                        },
                        decorator.range(),
                    ));
                }
            }

            if checker.enabled(Rule::PytestExtraneousScopeFunction) {
                let scope_keyword = keywords
                    .iter()
                    .find(|kw| kw.arg.as_ref().map_or(false, |arg| arg == "scope"));

                if let Some(scope_keyword) = scope_keyword {
                    if keyword_is_literal(scope_keyword, "function") {
                        let mut diagnostic =
                            Diagnostic::new(PytestExtraneousScopeFunction, scope_keyword.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.try_set_fix(|| {
                                remove_argument(
                                    checker.locator,
                                    func.end(),
                                    scope_keyword.range,
                                    args,
                                    keywords,
                                    false,
                                )
                                .map(Fix::suggested)
                            });
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        _ => {
            if checker.enabled(Rule::PytestFixtureIncorrectParenthesesStyle) {
                if checker.settings.flake8_pytest_style.fixture_parentheses {
                    let fix = Fix::automatic(Edit::insertion(
                        Parentheses::Empty.to_string(),
                        decorator.end(),
                    ));
                    pytest_fixture_parentheses(
                        checker,
                        decorator,
                        fix,
                        Parentheses::Empty,
                        Parentheses::None,
                    );
                }
            }
        }
    }
}

/// PT004, PT005, PT022
fn check_fixture_returns(checker: &mut Checker, stmt: &Stmt, name: &str, body: &[Stmt]) {
    let mut visitor = SkipFunctionsVisitor::default();

    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if checker.enabled(Rule::PytestIncorrectFixtureNameUnderscore)
        && visitor.has_return_with_value
        && name.starts_with('_')
    {
        checker.diagnostics.push(Diagnostic::new(
            PytestIncorrectFixtureNameUnderscore {
                function: name.to_string(),
            },
            stmt.identifier(),
        ));
    } else if checker.enabled(Rule::PytestMissingFixtureNameUnderscore)
        && !visitor.has_return_with_value
        && !visitor.has_yield_from
        && !name.starts_with('_')
    {
        checker.diagnostics.push(Diagnostic::new(
            PytestMissingFixtureNameUnderscore {
                function: name.to_string(),
            },
            stmt.identifier(),
        ));
    }

    if checker.enabled(Rule::PytestUselessYieldFixture) {
        if let Some(stmt) = body.last() {
            if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt {
                if value.is_yield_expr() {
                    if visitor.yield_statements.len() == 1 {
                        let mut diagnostic = Diagnostic::new(
                            PytestUselessYieldFixture {
                                name: name.to_string(),
                            },
                            stmt.range(),
                        );
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                                "return".to_string(),
                                TextRange::at(stmt.start(), "yield".text_len()),
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }
}

/// PT019
fn check_test_function_args(checker: &mut Checker, arguments: &Arguments) {
    arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
        .for_each(
            |ArgWithDefault {
                 def,
                 default: _,
                 range: _,
             }| {
                let name = &def.arg;
                if name.starts_with('_') {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestFixtureParamWithoutValue {
                            name: name.to_string(),
                        },
                        def.range(),
                    ));
                }
            },
        );
}

/// PT020
fn check_fixture_decorator_name(checker: &mut Checker, decorator: &Decorator) {
    if is_pytest_yield_fixture(decorator, checker.semantic()) {
        checker.diagnostics.push(Diagnostic::new(
            PytestDeprecatedYieldFixture,
            decorator.range(),
        ));
    }
}

/// PT021
fn check_fixture_addfinalizer(checker: &mut Checker, args: &Arguments, body: &[Stmt]) {
    if !includes_arg_name("request", args) {
        return;
    }

    let mut visitor = SkipFunctionsVisitor::default();

    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if let Some(addfinalizer) = visitor.addfinalizer_call {
        checker.diagnostics.push(Diagnostic::new(
            PytestFixtureFinalizerCallback,
            addfinalizer.range(),
        ));
    }
}

/// PT024, PT025
fn check_fixture_marks(checker: &mut Checker, decorators: &[Decorator]) {
    for (expr, call_path) in get_mark_decorators(decorators) {
        let name = call_path.last().expect("Expected a mark name");
        if checker.enabled(Rule::PytestUnnecessaryAsyncioMarkOnFixture) {
            if *name == "asyncio" {
                let mut diagnostic =
                    Diagnostic::new(PytestUnnecessaryAsyncioMarkOnFixture, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    let range = checker.locator.full_lines_range(expr.range());
                    diagnostic.set_fix(Fix::automatic(Edit::range_deletion(range)));
                }
                checker.diagnostics.push(diagnostic);
            }
        }

        if checker.enabled(Rule::PytestErroneousUseFixturesOnFixture) {
            if *name == "usefixtures" {
                let mut diagnostic =
                    Diagnostic::new(PytestErroneousUseFixturesOnFixture, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    let line_range = checker.locator.full_lines_range(expr.range());
                    diagnostic.set_fix(Fix::automatic(Edit::range_deletion(line_range)));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

pub(crate) fn fixture(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    args: &Arguments,
    decorators: &[Decorator],
    body: &[Stmt],
) {
    let decorator = fixture_decorator(decorators, checker.semantic());
    if let Some(decorator) = decorator {
        if checker.enabled(Rule::PytestFixtureIncorrectParenthesesStyle)
            || checker.enabled(Rule::PytestFixturePositionalArgs)
            || checker.enabled(Rule::PytestExtraneousScopeFunction)
        {
            check_fixture_decorator(checker, name, decorator);
        }

        if checker.enabled(Rule::PytestDeprecatedYieldFixture)
            && checker.settings.flake8_pytest_style.fixture_parentheses
        {
            check_fixture_decorator_name(checker, decorator);
        }

        if (checker.enabled(Rule::PytestMissingFixtureNameUnderscore)
            || checker.enabled(Rule::PytestIncorrectFixtureNameUnderscore)
            || checker.enabled(Rule::PytestUselessYieldFixture))
            && !is_abstract(decorators, checker.semantic())
        {
            check_fixture_returns(checker, stmt, name, body);
        }

        if checker.enabled(Rule::PytestFixtureFinalizerCallback) {
            check_fixture_addfinalizer(checker, args, body);
        }

        if checker.enabled(Rule::PytestUnnecessaryAsyncioMarkOnFixture)
            || checker.enabled(Rule::PytestErroneousUseFixturesOnFixture)
        {
            check_fixture_marks(checker, decorators);
        }
    }

    if checker.enabled(Rule::PytestFixtureParamWithoutValue) && name.starts_with("test_") {
        check_test_function_args(checker, args);
    }
}
