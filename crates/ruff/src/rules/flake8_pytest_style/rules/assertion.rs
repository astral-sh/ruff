use std::borrow::Cow;

use anyhow::bail;
use anyhow::Result;
use libcst_native::{
    self, Assert, BooleanOp, CompoundStatement, Expression, ParenthesizableWhitespace,
    ParenthesizedNode, SimpleStatementLine, SimpleWhitespace, SmallStatement, Statement,
    TrailingWhitespace, UnaryOperation,
};
use rustpython_parser::ast::{self, BoolOp, ExceptHandler, Expr, Keyword, Ranged, Stmt, UnaryOp};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_comments_in, Truthiness};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{visitor, whitespace};

use crate::autofix::codemods::CodegenStylist;
use crate::checkers::ast::Checker;
use crate::cst::matchers::match_indented_block;
use crate::cst::matchers::match_module;
use crate::registry::AsRule;

use super::unittest_assert::UnittestAssert;

/// ## What it does
/// Checks for assertions that combine multiple independent conditions.
///
/// ## Why is this bad?
/// Composite assertion statements are harder debug upon failure, as the
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
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Break down assertion into multiple parts".to_string())
    }
}

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

#[violation]
pub struct PytestAssertAlwaysFalse;

impl Violation for PytestAssertAlwaysFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion always fails, replace with `pytest.fail()`")
    }
}

#[violation]
pub struct PytestUnittestAssertion {
    assertion: String,
}

impl Violation for PytestUnittestAssertion {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestUnittestAssertion { assertion } = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn autofix_title(&self) -> Option<String> {
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

impl<'a, 'b> Visitor<'b> for ExceptionHandlerVisitor<'a>
where
    'b: 'a,
{
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
                if checker.patch(diagnostic.kind.rule()) {
                    // We're converting an expression to a statement, so avoid applying the fix if
                    // the assertion is part of a larger expression.
                    if checker.semantic().stmt().is_expr_stmt()
                        && checker.semantic().expr_parent().is_none()
                        && !checker.semantic().scope().kind.is_lambda()
                        && !has_comments_in(expr.range(), checker.locator)
                    {
                        if let Ok(stmt) = unittest_assert.generate_assert(args, keywords) {
                            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                                checker.generator().stmt(&stmt),
                                expr.range(),
                            )));
                        }
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

/// PT015
pub(crate) fn assert_falsy(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if Truthiness::from_expr(test, |id| checker.semantic().is_builtin(id)).is_falsey() {
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

/// Negate a condition, i.e., `a` => `not a` and `not a` => `a`.
fn negate<'a>(expression: &Expression<'a>) -> Expression<'a> {
    if let Expression::UnaryOperation(ref expression) = expression {
        if matches!(expression.operator, libcst_native::UnaryOp::Not { .. }) {
            return *expression.expression.clone();
        }
    }
    Expression::UnaryOperation(Box::new(UnaryOperation {
        operator: libcst_native::UnaryOp::Not {
            whitespace_after: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
        },
        expression: Box::new(expression.clone()),
        lpar: vec![],
        rpar: vec![],
    }))
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
fn parenthesize<'a>(expression: Expression<'a>, parent: &Expression<'a>) -> Expression<'a> {
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
            return expression.with_parens(left.clone(), right.clone());
        }
    }
    expression
}

/// Replace composite condition `assert a == "hello" and b == "world"` with two statements
/// `assert a == "hello"` and `assert b == "world"`.
fn fix_composite_condition(stmt: &Stmt, locator: &Locator, stylist: &Stylist) -> Result<Edit> {
    // Infer the indentation of the outer block.
    let Some(outer_indent) = whitespace::indentation(locator, stmt) else {
        bail!("Unable to fix multiline statement");
    };

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

    let [Statement::Simple(simple_statement_line)] = &statements[..] else {
        bail!("Expected one simple statement")
    };

    let [SmallStatement::Assert(assert_statement)] = &simple_statement_line.body[..] else {
        bail!("Expected simple statement to be an assert")
    };

    // Extract the individual conditions.
    let mut conditions: Vec<Expression> = Vec::with_capacity(2);
    match &assert_statement.test {
        Expression::UnaryOperation(op) => {
            if matches!(op.operator, libcst_native::UnaryOp::Not { .. }) {
                if let Expression::BooleanOperation(op) = &*op.expression {
                    if matches!(op.operator, BooleanOp::Or { .. }) {
                        conditions.push(parenthesize(negate(&op.left), &assert_statement.test));
                        conditions.push(parenthesize(negate(&op.right), &assert_statement.test));
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
                conditions.push(parenthesize(*op.left.clone(), &assert_statement.test));
                conditions.push(parenthesize(*op.right.clone(), &assert_statement.test));
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
        if checker.patch(diagnostic.kind.rule()) {
            if matches!(composite, CompositionKind::Simple)
                && msg.is_none()
                && !has_comments_in(stmt.range(), checker.locator)
            {
                #[allow(deprecated)]
                diagnostic.try_set_fix_from_edit(|| {
                    fix_composite_condition(stmt, checker.locator, checker.stylist)
                });
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
