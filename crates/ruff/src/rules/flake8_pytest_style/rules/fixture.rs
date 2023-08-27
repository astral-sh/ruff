use std::fmt;

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::Decorator;
use ruff_python_ast::{self as ast, Expr, ParameterWithDefault, Parameters, Stmt};
use ruff_python_semantic::analyze::visibility::is_abstract;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::autofix::edits;
use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

use super::helpers::{
    get_mark_decorators, is_pytest_fixture, is_pytest_yield_fixture, keyword_is_literal,
};

/// ## What it does
/// Checks for argument-free `@pytest.fixture()` decorators with or without
/// parentheses, depending on the `flake8-pytest-style.fixture-parentheses`
/// setting.
///
/// ## Why is this bad?
/// If a `@pytext.fixture()` doesn't take any arguments, the parentheses are
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
/// @pytest.fixture
/// def my_fixture():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture():
///     ...
/// ```
///
/// ## Options
/// - `flake8-pytest-style.fixture-parentheses`
///
/// ## References
/// - [`pytest` documentation: API Reference: Fixtures](https://docs.pytest.org/en/latest/reference/reference.html#fixtures-api)
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

/// ## What it does
/// Checks for `pytest.fixture` calls with positional arguments.
///
/// ## Why is this bad?
/// For clarity and consistency, prefer using keyword arguments to specify
/// fixture configuration.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture("module")
/// def my_fixture():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture(scope="module")
/// def my_fixture():
///     ...
/// ```
///
/// ## References
/// - [`pytest` documentation: `@pytest.fixture` functions](https://docs.pytest.org/en/latest/reference/reference.html#pytest-fixture)
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

/// ## What it does
/// Checks for `pytest.fixture` calls with `scope="function"`.
///
/// ## Why is this bad?
/// `scope="function"` can be omitted, as it is the default.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture(scope="function")
/// def my_fixture():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture():
///     ...
/// ```
///
/// ## References
/// - [`pytest` documentation: `@pytest.fixture` functions](https://docs.pytest.org/en/latest/reference/reference.html#pytest-fixture)
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

/// ## What it does
/// Checks for `pytest` fixtures that do not return a value, but are not named
/// with a leading underscore.
///
/// ## Why is this bad?
/// By convention, fixtures that don't return a value should be named with a
/// leading underscore, while fixtures that do return a value should not.
///
/// This rule ignores abstract fixtures and generators.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def patch_something(mocker):
///     mocker.patch("module.object")
///
///
/// @pytest.fixture()
/// def use_context():
///     with create_context():
///         yield
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def _patch_something(mocker):
///     mocker.patch("module.object")
///
///
/// @pytest.fixture()
/// def _use_context():
///     with create_context():
///         yield
/// ```
///
/// ## References
/// - [`pytest` documentation: `@pytest.fixture` functions](https://docs.pytest.org/en/latest/reference/reference.html#pytest-fixture)
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

/// ## What it does
/// Checks for `pytest` fixtures that return a value, but are named with a
/// leading underscore.
///
/// ## Why is this bad?
/// By convention, fixtures that don't return a value should be named with a
/// leading underscore, while fixtures that do return a value should not.
///
/// This rule ignores abstract fixtures.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def _some_object():
///     return SomeClass()
///
///
/// @pytest.fixture()
/// def _some_object_with_cleanup():
///     obj = SomeClass()
///     yield obj
///     obj.cleanup()
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def some_object():
///     return SomeClass()
///
///
/// @pytest.fixture()
/// def some_object_with_cleanup():
///     obj = SomeClass()
///     yield obj
///     obj.cleanup()
/// ```
///
/// ## References
/// - [`pytest` documentation: `@pytest.fixture` functions](https://docs.pytest.org/en/latest/reference/reference.html#pytest-fixture)
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

/// ## What it does
/// Checks for `pytest` test functions that should be decorated with
/// `@pytest.mark.usefixtures`.
///
/// ## Why is this bad?
/// In `pytest`, fixture injection is used to activate fixtures in a test
/// function.
///
/// Fixtures can be injected either by passing them as parameters to the test
/// function, or by using the `@pytest.mark.usefixtures` decorator.
///
/// If the test function depends on the fixture being activated, but does not
/// use it in the test body or otherwise rely on its return value, prefer
/// the `@pytest.mark.usefixtures` decorator, to make the dependency explicit
/// and avoid the confusion caused by unused arguments.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture
/// def _patch_something():
///     ...
///
///
/// def test_foo(_patch_something):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture
/// def _patch_something():
///     ...
///
///
/// @pytest.mark.usefixtures("_patch_something")
/// def test_foo():
///     ...
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.mark.usefixtures`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-mark-usefixtures)
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

/// ## What it does
/// Checks for `pytest.yield_fixture` usage.
///
/// ## Why is this bad?
/// `pytest.yield_fixture` is deprecated. `pytest.fixture` should be used instead.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.yield_fixture()
/// def my_fixture():
///     obj = SomeClass()
///     yield obj
///     obj.cleanup()
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture():
///     obj = SomeClass()
///     yield obj
///     obj.cleanup()
/// ```
///
/// ## References
/// - [`pytest` documentation: `yield_fixture` functions](https://docs.pytest.org/en/latest/yieldfixture.html)
#[violation]
pub struct PytestDeprecatedYieldFixture;

impl Violation for PytestDeprecatedYieldFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`@pytest.yield_fixture` is deprecated, use `@pytest.fixture`")
    }
}

/// ## What it does
/// Checks for unnecessary `request.addfinalizer` usages in `pytest` fixtures.
///
/// ## Why is this bad?
/// `pytest` offers two ways to perform cleanup in fixture code. The first is
/// sequential (via the `yield` statement), the second callback-based (via
/// `request.addfinalizer`).
///
/// The sequential approach is more readable and should be preferred, unless
/// the fixture uses the "factory as fixture" pattern.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture(request):
///     resource = acquire_resource()
///     request.addfinalizer(resource.release)
///     return resource
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture():
///     resource = acquire_resource()
///     yield resource
///     resource.release()
///
///
/// # "factory-as-fixture" pattern
/// @pytest.fixture()
/// def my_factory(request):
///     def create_resource(arg):
///         resource = acquire_resource(arg)
///         request.addfinalizer(resource.release)
///         return resource
///
///     return create_resource
/// ```
///
/// ## References
/// - [`pytest` documentation: Adding finalizers directly](https://docs.pytest.org/en/latest/how-to/fixtures.html#adding-finalizers-directly)
/// - [`pytest` documentation: Factories as fixtures](https://docs.pytest.org/en/latest/how-to/fixtures.html#factories-as-fixtures)
#[violation]
pub struct PytestFixtureFinalizerCallback;

impl Violation for PytestFixtureFinalizerCallback {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `yield` instead of `request.addfinalizer`")
    }
}
/// ## What it does
/// Checks for unnecessary `yield` expressions in `pytest` fixtures.
///
/// ## Why is this bad?
/// In `pytest` fixtures, the `yield` expression should only be used for fixtures
/// that include teardown code, to clean up the fixture after the test function
/// has finished executing.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture():
///     resource = acquire_resource()
///     yield resource
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture_with_teardown():
///     resource = acquire_resource()
///     yield resource
///     resource.release()
///
///
/// @pytest.fixture()
/// def my_fixture_without_teardown():
///     resource = acquire_resource()
///     return resource
/// ```
///
/// ## References
/// - [`pytest` documentation: Teardown/Cleanup](https://docs.pytest.org/en/latest/how-to/fixtures.html#teardown-cleanup-aka-fixture-finalization)
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

/// ## What it does
/// Checks for `pytest.mark.usefixtures` decorators applied to `pytest`
/// fixtures.
///
/// ## Why is this bad?
/// The `pytest.mark.usefixtures` decorator has no effect on `pytest` fixtures.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def a():
///     pass
///
///
/// @pytest.mark.usefixtures("a")
/// @pytest.fixture()
/// def b(a):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def a():
///     pass
///
///
/// @pytest.fixture()
/// def b(a):
///     pass
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.mark.usefixtures`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-mark-usefixtures)
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

/// ## What it does
/// Checks for unnecessary `@pytest.mark.asyncio` decorators applied to fixtures.
///
/// ## Why is this bad?
/// `pytest.mark.asyncio` is unnecessary for fixtures.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.mark.asyncio()
/// @pytest.fixture()
/// async def my_fixture():
///     return 0
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// async def my_fixture():
///     return 0
/// ```
///
/// ## References
/// - [`pytest-asyncio`](https://pypi.org/project/pytest-asyncio/)
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
            Stmt::FunctionDef(_) => {}
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
                if collect_call_path(func).is_some_and(|call_path| {
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
            arguments,
            range: _,
        }) => {
            if checker.enabled(Rule::PytestFixtureIncorrectParenthesesStyle) {
                if !checker.settings.flake8_pytest_style.fixture_parentheses
                    && arguments.args.is_empty()
                    && arguments.keywords.is_empty()
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
                if !arguments.args.is_empty() {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestFixturePositionalArgs {
                            function: func_name.to_string(),
                        },
                        decorator.range(),
                    ));
                }
            }

            if checker.enabled(Rule::PytestExtraneousScopeFunction) {
                if let Some(keyword) = arguments.find_keyword("scope") {
                    if keyword_is_literal(keyword, "function") {
                        let mut diagnostic =
                            Diagnostic::new(PytestExtraneousScopeFunction, keyword.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.try_set_fix(|| {
                                edits::remove_argument(
                                    keyword,
                                    arguments,
                                    edits::Parentheses::Preserve,
                                    checker.locator().contents(),
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
fn check_test_function_args(checker: &mut Checker, parameters: &Parameters) {
    parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
        .for_each(
            |ParameterWithDefault {
                 parameter,
                 default: _,
                 range: _,
             }| {
                let name = &parameter.name;
                if name.starts_with('_') {
                    checker.diagnostics.push(Diagnostic::new(
                        PytestFixtureParamWithoutValue {
                            name: name.to_string(),
                        },
                        parameter.range(),
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
fn check_fixture_addfinalizer(checker: &mut Checker, parameters: &Parameters, body: &[Stmt]) {
    if !parameters.includes("request") {
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
                    let range = checker.locator().full_lines_range(expr.range());
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
                    let line_range = checker.locator().full_lines_range(expr.range());
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
    parameters: &Parameters,
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
            check_fixture_addfinalizer(checker, parameters, body);
        }

        if checker.enabled(Rule::PytestUnnecessaryAsyncioMarkOnFixture)
            || checker.enabled(Rule::PytestErroneousUseFixturesOnFixture)
        {
            check_fixture_marks(checker, decorators);
        }
    }

    if checker.enabled(Rule::PytestFixtureParamWithoutValue) && name.starts_with("test_") {
        check_test_function_args(checker, parameters);
    }
}
