use std::borrow::Cow;

use anyhow::Result;
use anyhow::{bail, Context};
use libcst_native::{
    self, Assert, BooleanOp, CompoundStatement, Expression, ParenthesizedNode, SimpleStatementLine,
    SimpleWhitespace, SmallStatement, Statement, TrailingWhitespace,
};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    self as ast, Arguments, BoolOp, ExceptHandler, Expr, Keyword, Stmt, UnaryOp,
};
use ruff_python_ast::{visitor, whitespace};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::helpers::negate;
use crate::cst::matchers::match_indented_block;
use crate::cst::matchers::match_module;
use crate::fix::codemods::CodegenStylist;
use crate::importer::ImportRequest;

use super::unittest_assert::UnittestAssert;

/// ## What it does
/// Checks for assertions that combine multiple independent conditions.
///
/// ## Why is this bad?
/// Composite assertion statements are harder to debug upon failure, as the
/// failure message will not indicate which condition failed.
///
/// ## Example
/// ```python
/// def test_foo():
///     assert something and something_else
///
///
/// def test_bar():
///     assert not (something or something_else)
/// ```
///
/// Use instead:
/// ```python
/// def test_foo():
///     assert something
///     assert something_else
///
///
/// def test_bar():
///     assert not something
///     assert not something_else
/// ```
#[violation]
pub struct PytestCompositeAssertion;

impl Violation for PytestCompositeAssertion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Break down assertion into multiple parts".to_string())
    }
}

/// ## What it does
/// Checks for `assert` statements in `except` clauses.
///
/// ## Why is this bad?
/// When testing for exceptions, `pytest.raises()` should be used instead of
/// `assert` statements in `except` clauses, as it's more explicit and
/// idiomatic. Further, `pytest.raises()` will fail if the exception is _not_
/// raised, unlike the `assert` statement.
///
/// ## Example
/// ```python
/// def test_foo():
///     try:
///         1 / 0
///     except ZeroDivisionError as e:
///         assert e.args
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     with pytest.raises(ZeroDivisionError) as exc_info:
///         1 / 0
///     assert exc_info.value.args
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[violation]
pub struct PytestAssertInExcept {
    name: String,
}

impl Violation for PytestAssertInExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestAssertInExcept { name } = self;
        format!(
            "Found assertion on exception `{name}` in `except` block, use `pytest.raises()` instead"
        )
    }
}

/// ## What it does
/// Checks for `assert` statements whose test expression is a falsy value.
///
/// ## Why is this bad?
/// `pytest.fail` conveys the intent more clearly than `assert falsy_value`.
///
/// ## Example
/// ```python
/// def test_foo():
///     if some_condition:
///         assert False, "some_condition was True"
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     if some_condition:
///         pytest.fail("some_condition was True")
///     ...
/// ```
///
/// References
/// - [`pytest` documentation: `pytest.fail`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-fail)
#[violation]
pub struct PytestAssertAlwaysFalse;

impl Violation for PytestAssertAlwaysFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion always fails, replace with `pytest.fail()`")
    }
}

/// ## What it does
/// Checks for uses of assertion methods from the `unittest` module.
///
/// ## Why is this bad?
/// To make use of `pytest`'s assertion rewriting, a regular `assert` statement
/// is preferred over `unittest`'s assertion methods.
///
/// ## Example
/// ```python
/// import unittest
///
///
/// class TestFoo(unittest.TestCase):
///     def test_foo(self):
///         self.assertEqual(a, b)
/// ```
///
/// Use instead:
/// ```python
/// import unittest
///
///
/// class TestFoo(unittest.TestCase):
///     def test_foo(self):
///         assert a == b
/// ```
///
/// ## References
/// - [`pytest` documentation: Assertion introspection details](https://docs.pytest.org/en/7.1.x/how-to/assert.html#assertion-introspection-details)

#[violation]
pub struct PytestUnittestAssertion {
    assertion: String,
}

impl Violation for PytestUnittestAssertion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestUnittestAssertion { assertion } = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn fix_title(&self) -> Option<String> {
        let PytestUnittestAssertion { assertion } = self;
        Some(format!("Replace `{assertion}(...)` with `assert ...`"))
    }
}

/// Visitor that tracks assert statements and checks if they reference
/// the exception name.
struct ExceptionHandlerVisitor<'a> {
    exception_name: &'a str,
    current_assert: Option<&'a Stmt>,
    errors: Vec<Diagnostic>,
}

impl<'a> ExceptionHandlerVisitor<'a> {
    const fn new(exception_name: &'a str) -> Self {
        Self {
            exception_name,
            current_assert: None,
            errors: Vec::new(),
        }
    }
}

impl<'a> Visitor<'a> for ExceptionHandlerVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Assert(_) => {
                self.current_assert = Some(stmt);
                visitor::walk_stmt(self, stmt);
                self.current_assert = None;
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                if let Some(current_assert) = self.current_assert {
                    if id.as_str() == self.exception_name {
                        self.errors.push(Diagnostic::new(
                            PytestAssertInExcept {
                                name: id.to_string(),
                            },
                            current_assert.range(),
                        ));
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

fn check_assert_in_except(name: &str, body: &[Stmt]) -> Vec<Diagnostic> {
    // Walk body to find assert statements that reference the exception name
    let mut visitor = ExceptionHandlerVisitor::new(name);
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
    visitor.errors
}

/// PT009
pub(crate) fn unittest_assertion(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    match func {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            if let Ok(unittest_assert) = UnittestAssert::try_from(attr.as_str()) {
                let mut diagnostic = Diagnostic::new(
                    PytestUnittestAssertion {
                        assertion: unittest_assert.to_string(),
                    },
                    func.range(),
                );
                // We're converting an expression to a statement, so avoid applying the fix if
                // the assertion is part of a larger expression.
                if checker.semantic().current_statement().is_expr_stmt()
                    && checker.semantic().current_expression_parent().is_none()
                    && !checker.comment_ranges().intersects(expr.range())
                {
                    if let Ok(stmt) = unittest_assert.generate_assert(args, keywords) {
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                            checker.generator().stmt(&stmt),
                            parenthesized_range(
                                expr.into(),
                                checker.semantic().current_statement().into(),
                                checker.comment_ranges(),
                                checker.locator().contents(),
                            )
                            .unwrap_or(expr.range()),
                        )));
                    }
                }
                Some(diagnostic)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// ## What it does
/// Checks for uses of exception-related assertion methods from the `unittest`
/// module.
///
/// ## Why is this bad?
/// To enforce the assertion style recommended by `pytest`, `pytest.raises` is
/// preferred over the exception-related assertion methods in `unittest`, like
/// `assertRaises`.
///
/// ## Example
/// ```python
/// import unittest
///
///
/// class TestFoo(unittest.TestCase):
///     def test_foo(self):
///         with self.assertRaises(ValueError):
///             raise ValueError("foo")
/// ```
///
/// Use instead:
/// ```python
/// import unittest
/// import pytest
///
///
/// class TestFoo(unittest.TestCase):
///     def test_foo(self):
///         with pytest.raises(ValueError):
///             raise ValueError("foo")
/// ```
///
/// ## References
/// - [`pytest` documentation: Assertions about expected exceptions](https://docs.pytest.org/en/latest/how-to/assert.html#assertions-about-expected-exceptions)
#[violation]
pub struct PytestUnittestRaisesAssertion {
    assertion: String,
}

impl Violation for PytestUnittestRaisesAssertion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestUnittestRaisesAssertion { assertion } = self;
        format!("Use `pytest.raises` instead of unittest-style `{assertion}`")
    }

    fn fix_title(&self) -> Option<String> {
        let PytestUnittestRaisesAssertion { assertion } = self;
        Some(format!("Replace `{assertion}` with `pytest.raises`"))
    }
}

/// PT027
pub(crate) fn unittest_raises_assertion(
    checker: &Checker,
    call: &ast::ExprCall,
) -> Option<Diagnostic> {
    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = call.func.as_ref() else {
        return None;
    };

    if !matches!(
        attr.as_str(),
        "assertRaises" | "failUnlessRaises" | "assertRaisesRegex" | "assertRaisesRegexp"
    ) {
        return None;
    }

    let mut diagnostic = Diagnostic::new(
        PytestUnittestRaisesAssertion {
            assertion: attr.to_string(),
        },
        call.func.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(call, checker.locator())
    {
        if let Some(args) = to_pytest_raises_args(checker, attr.as_str(), &call.arguments) {
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import("pytest", "raises"),
                    call.func.start(),
                    checker.semantic(),
                )?;
                let edit = Edit::range_replacement(format!("{binding}({args})"), call.range());
                Ok(Fix::unsafe_edits(import_edit, [edit]))
            });
        }
    }

    Some(diagnostic)
}

fn to_pytest_raises_args<'a>(
    checker: &Checker<'a>,
    attr: &str,
    arguments: &Arguments,
) -> Option<Cow<'a, str>> {
    let args = match attr {
        "assertRaises" | "failUnlessRaises" => {
            match (&*arguments.args, &*arguments.keywords) {
                // Ex) `assertRaises(Exception)`
                ([arg], []) => Cow::Borrowed(checker.locator().slice(arg)),
                // Ex) `assertRaises(expected_exception=Exception)`
                ([], [kwarg])
                    if kwarg
                        .arg
                        .as_ref()
                        .is_some_and(|id| id.as_str() == "expected_exception") =>
                {
                    Cow::Borrowed(checker.locator().slice(kwarg.value.range()))
                }
                _ => return None,
            }
        }
        "assertRaisesRegex" | "assertRaisesRegexp" => {
            match (&*arguments.args, &*arguments.keywords) {
                // Ex) `assertRaisesRegex(Exception, regex)`
                ([arg1, arg2], []) => Cow::Owned(format!(
                    "{}, match={}",
                    checker.locator().slice(arg1),
                    checker.locator().slice(arg2)
                )),
                // Ex) `assertRaisesRegex(Exception, expected_regex=regex)`
                ([arg], [kwarg])
                    if kwarg
                        .arg
                        .as_ref()
                        .is_some_and(|arg| arg.as_str() == "expected_regex") =>
                {
                    Cow::Owned(format!(
                        "{}, match={}",
                        checker.locator().slice(arg),
                        checker.locator().slice(kwarg.value.range())
                    ))
                }
                // Ex) `assertRaisesRegex(expected_exception=Exception, expected_regex=regex)`
                ([], [kwarg1, kwarg2])
                    if kwarg1
                        .arg
                        .as_ref()
                        .is_some_and(|id| id.as_str() == "expected_exception")
                        && kwarg2
                            .arg
                            .as_ref()
                            .is_some_and(|id| id.as_str() == "expected_regex") =>
                {
                    Cow::Owned(format!(
                        "{}, match={}",
                        checker.locator().slice(kwarg1.value.range()),
                        checker.locator().slice(kwarg2.value.range())
                    ))
                }
                // Ex) `assertRaisesRegex(expected_regex=regex, expected_exception=Exception)`
                ([], [kwarg1, kwarg2])
                    if kwarg1
                        .arg
                        .as_ref()
                        .is_some_and(|id| id.as_str() == "expected_regex")
                        && kwarg2
                            .arg
                            .as_ref()
                            .is_some_and(|id| id.as_str() == "expected_exception") =>
                {
                    Cow::Owned(format!(
                        "{}, match={}",
                        checker.locator().slice(kwarg2.value.range()),
                        checker.locator().slice(kwarg1.value.range())
                    ))
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(args)
}

/// PT015
pub(crate) fn assert_falsy(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    let truthiness = Truthiness::from_expr(test, |id| checker.semantic().has_builtin_binding(id));
    if matches!(truthiness, Truthiness::False | Truthiness::Falsey) {
        checker
            .diagnostics
            .push(Diagnostic::new(PytestAssertAlwaysFalse, stmt.range()));
    }
}

/// PT017
pub(crate) fn assert_in_exception_handler(checker: &mut Checker, handlers: &[ExceptHandler]) {
    checker
        .diagnostics
        .extend(handlers.iter().flat_map(|handler| match handler {
            ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                name, body, ..
            }) => {
                if let Some(name) = name {
                    check_assert_in_except(name, body)
                } else {
                    Vec::new()
                }
            }
        }));
}

#[derive(Copy, Clone)]
enum CompositionKind {
    // E.g., `a or b or c`.
    None,
    // E.g., `a and b` or `not (a or b)`.
    Simple,
    // E.g., `not (a and b or c)`.
    Mixed,
}

/// Check if the test expression is a composite condition, and whether it can
/// be split into multiple independent conditions.
///
/// For example, `a and b` or `not (a or b)`. The latter is equivalent to
/// `not a and not b` by De Morgan's laws.
fn is_composite_condition(test: &Expr) -> CompositionKind {
    match test {
        Expr::BoolOp(ast::ExprBoolOp {
            op: BoolOp::And, ..
        }) => {
            return CompositionKind::Simple;
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::Not,
            operand,
            range: _,
        }) => {
            if let Expr::BoolOp(ast::ExprBoolOp {
                op: BoolOp::Or,
                values,
                range: _,
            }) = operand.as_ref()
            {
                // Only split cases without mixed `and` and `or`.
                return if values.iter().all(|expr| {
                    !matches!(
                        expr,
                        Expr::BoolOp(ast::ExprBoolOp {
                            op: BoolOp::And,
                            ..
                        })
                    )
                }) {
                    CompositionKind::Simple
                } else {
                    CompositionKind::Mixed
                };
            }
        }
        _ => {}
    }
    CompositionKind::None
}

/// Propagate parentheses from a parent to a child expression, if necessary.
///
/// For example, when splitting:
/// ```python
/// assert (a and b ==
///     """)
/// ```
///
/// The parentheses need to be propagated to the right-most expression:
/// ```python
/// assert a
/// assert (b ==
///     "")
/// ```
fn parenthesize<'a>(expression: &Expression<'a>, parent: &Expression<'a>) -> Expression<'a> {
    if matches!(
        expression,
        Expression::Comparison(_)
            | Expression::UnaryOperation(_)
            | Expression::BinaryOperation(_)
            | Expression::BooleanOperation(_)
            | Expression::Attribute(_)
            | Expression::Tuple(_)
            | Expression::Call(_)
            | Expression::GeneratorExp(_)
            | Expression::ListComp(_)
            | Expression::SetComp(_)
            | Expression::DictComp(_)
            | Expression::List(_)
            | Expression::Set(_)
            | Expression::Dict(_)
            | Expression::Subscript(_)
            | Expression::StarredElement(_)
            | Expression::IfExp(_)
            | Expression::Lambda(_)
            | Expression::Yield(_)
            | Expression::Await(_)
            | Expression::ConcatenatedString(_)
            | Expression::FormattedString(_)
            | Expression::NamedExpr(_)
    ) {
        if let (Some(left), Some(right)) = (parent.lpar().first(), parent.rpar().first()) {
            return expression.clone().with_parens(left.clone(), right.clone());
        }
    }
    expression.clone()
}

/// Replace composite condition `assert a == "hello" and b == "world"` with two statements
/// `assert a == "hello"` and `assert b == "world"`.
fn fix_composite_condition(stmt: &Stmt, locator: &Locator, stylist: &Stylist) -> Result<Edit> {
    // Infer the indentation of the outer block.
    let outer_indent =
        whitespace::indentation(locator, stmt).context("Unable to fix multiline statement")?;

    // Extract the module text.
    let contents = locator.lines(stmt.range());

    // If the block is indented, "embed" it in a function definition, to preserve
    // indentation while retaining valid source code. (We'll strip the prefix later
    // on.)
    let module_text = if outer_indent.is_empty() {
        Cow::Borrowed(contents)
    } else {
        Cow::Owned(format!(
            "def f():{}{contents}",
            stylist.line_ending().as_str()
        ))
    };

    // Parse the CST.
    let mut tree = match_module(&module_text)?;

    // Extract the assert statement.
    let statements = if outer_indent.is_empty() {
        &mut tree.body
    } else {
        let [Statement::Compound(CompoundStatement::FunctionDef(embedding))] = &mut *tree.body
        else {
            bail!("Expected statement to be embedded in a function definition")
        };

        let indented_block = match_indented_block(&mut embedding.body)?;
        indented_block.indent = Some(outer_indent);

        &mut indented_block.body
    };

    let [Statement::Simple(simple_statement_line)] = statements.as_slice() else {
        bail!("Expected one simple statement")
    };

    let [SmallStatement::Assert(assert_statement)] = simple_statement_line.body.as_slice() else {
        bail!("Expected simple statement to be an assert")
    };

    // Extract the individual conditions.
    let mut conditions: Vec<Expression> = Vec::with_capacity(2);
    match &assert_statement.test {
        Expression::UnaryOperation(op) => {
            if matches!(op.operator, libcst_native::UnaryOp::Not { .. }) {
                if let Expression::BooleanOperation(boolean_operation) = &*op.expression {
                    if matches!(boolean_operation.operator, BooleanOp::Or { .. }) {
                        conditions.push(negate(&parenthesize(
                            &boolean_operation.left,
                            &op.expression,
                        )));
                        conditions.push(negate(&parenthesize(
                            &boolean_operation.right,
                            &op.expression,
                        )));
                    } else {
                        bail!("Expected assert statement to be a composite condition");
                    }
                } else {
                    bail!("Expected assert statement to be a composite condition");
                }
            }
        }
        Expression::BooleanOperation(op) => {
            if matches!(op.operator, BooleanOp::And { .. }) {
                conditions.push(parenthesize(&op.left, &assert_statement.test));
                conditions.push(parenthesize(&op.right, &assert_statement.test));
            } else {
                bail!("Expected assert statement to be a composite condition");
            }
        }
        _ => bail!("Expected assert statement to be a composite condition"),
    }

    // For each condition, create an `assert condition` statement.
    statements.clear();
    for condition in conditions {
        statements.push(Statement::Simple(SimpleStatementLine {
            body: vec![SmallStatement::Assert(Assert {
                test: condition,
                msg: None,
                comma: None,
                whitespace_after_assert: SimpleWhitespace(" "),
                semicolon: None,
            })],
            leading_lines: Vec::default(),
            trailing_whitespace: TrailingWhitespace::default(),
        }));
    }

    // Reconstruct and reformat the code.
    let module_text = tree.codegen_stylist(stylist);
    let contents = if outer_indent.is_empty() {
        module_text
    } else {
        module_text
            .strip_prefix(&format!("def f():{}", stylist.line_ending().as_str()))
            .unwrap()
            .to_string()
    };

    let range = locator.full_lines_range(stmt.range());

    Ok(Edit::range_replacement(contents, range))
}

/// PT018
pub(crate) fn composite_condition(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    msg: Option<&Expr>,
) {
    let composite = is_composite_condition(test);
    if matches!(composite, CompositionKind::Simple | CompositionKind::Mixed) {
        let mut diagnostic = Diagnostic::new(PytestCompositeAssertion, stmt.range());
        if matches!(composite, CompositionKind::Simple)
            && msg.is_none()
            && !checker.comment_ranges().intersects(stmt.range())
            && !checker
                .indexer()
                .in_multi_statement_line(stmt, checker.locator())
        {
            diagnostic.try_set_fix(|| {
                fix_composite_condition(stmt, checker.locator(), checker.stylist())
                    .map(Fix::unsafe_edit)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
