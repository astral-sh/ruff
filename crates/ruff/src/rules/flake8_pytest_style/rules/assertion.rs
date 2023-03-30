use anyhow::bail;
use anyhow::Result;
use libcst_native::{
    Assert, BooleanOp, Codegen, CodegenState, CompoundStatement, Expression,
    ParenthesizableWhitespace, ParenthesizedNode, SimpleStatementLine, SimpleWhitespace,
    SmallStatement, Statement, Suite, TrailingWhitespace, UnaryOp, UnaryOperation,
};
use rustpython_parser::ast::{
    Boolop, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword, Location, Stmt, StmtKind,
    Unaryop,
};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_comments_in, unparse_stmt};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{visitor, whitespace};

use crate::checkers::ast::Checker;
use crate::cst::matchers::match_module;
use crate::registry::AsRule;

use super::helpers::is_falsy_constant;
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
pub struct PytestCompositeAssertion {
    pub fixable: bool,
}

impl Violation for PytestCompositeAssertion {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|_| format!("Break down assertion into multiple parts"))
    }
}

#[violation]
pub struct PytestAssertInExcept {
    pub name: String,
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
    pub assertion: String,
    pub fixable: bool,
}

impl Violation for PytestUnittestAssertion {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PytestUnittestAssertion { assertion, .. } = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|PytestUnittestAssertion { assertion, .. }| {
                format!("Replace `{assertion}(...)` with `assert ...`")
            })
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
        match &stmt.node {
            StmtKind::Assert { .. } => {
                self.current_assert = Some(stmt);
                visitor::walk_stmt(self, stmt);
                self.current_assert = None;
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Name { id, .. } => {
                if let Some(current_assert) = self.current_assert {
                    if id.as_str() == self.exception_name {
                        self.errors.push(Diagnostic::new(
                            PytestAssertInExcept {
                                name: id.to_string(),
                            },
                            Range::from(current_assert),
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
pub fn unittest_assertion(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    match &func.node {
        ExprKind::Attribute { attr, .. } => {
            if let Ok(unittest_assert) = UnittestAssert::try_from(attr.as_str()) {
                // We're converting an expression to a statement, so avoid applying the fix if
                // the assertion is part of a larger expression.
                let fixable = checker.ctx.current_expr_parent().is_none()
                    && matches!(checker.ctx.current_stmt().node, StmtKind::Expr { .. })
                    && !has_comments_in(Range::from(expr), checker.locator);
                let mut diagnostic = Diagnostic::new(
                    PytestUnittestAssertion {
                        assertion: unittest_assert.to_string(),
                        fixable,
                    },
                    Range::from(func),
                );
                if fixable && checker.patch(diagnostic.kind.rule()) {
                    if let Ok(stmt) = unittest_assert.generate_assert(args, keywords) {
                        diagnostic.set_fix(Edit::replacement(
                            unparse_stmt(&stmt, checker.stylist),
                            expr.location,
                            expr.end_location.unwrap(),
                        ));
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
pub fn assert_falsy(stmt: &Stmt, test: &Expr) -> Option<Diagnostic> {
    if is_falsy_constant(test) {
        Some(Diagnostic::new(PytestAssertAlwaysFalse, Range::from(stmt)))
    } else {
        None
    }
}

/// PT017
pub fn assert_in_exception_handler(handlers: &[Excepthandler]) -> Vec<Diagnostic> {
    handlers
        .iter()
        .flat_map(|handler| match &handler.node {
            ExcepthandlerKind::ExceptHandler { name, body, .. } => {
                if let Some(name) = name {
                    check_assert_in_except(name, body)
                } else {
                    Vec::new()
                }
            }
        })
        .collect()
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
    match &test.node {
        ExprKind::BoolOp {
            op: Boolop::And, ..
        } => {
            return CompositionKind::Simple;
        }
        ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } => {
            if let ExprKind::BoolOp {
                op: Boolop::Or,
                values,
            } = &operand.node
            {
                // Only split cases without mixed `and` and `or`.
                return if values.iter().all(|expr| {
                    !matches!(
                        expr.node,
                        ExprKind::BoolOp {
                            op: Boolop::And,
                            ..
                        }
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
        if matches!(expression.operator, UnaryOp::Not { .. }) {
            return *expression.expression.clone();
        }
    }
    Expression::UnaryOperation(Box::new(UnaryOperation {
        operator: UnaryOp::Not {
            whitespace_after: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
        },
        expression: Box::new(expression.clone()),
        lpar: vec![],
        rpar: vec![],
    }))
}

/// Replace composite condition `assert a == "hello" and b == "world"` with two statements
/// `assert a == "hello"` and `assert b == "world"`.
fn fix_composite_condition(stmt: &Stmt, locator: &Locator, stylist: &Stylist) -> Result<Edit> {
    // Infer the indentation of the outer block.
    let Some(outer_indent) = whitespace::indentation(locator, stmt) else {
        bail!("Unable to fix multiline statement");
    };

    // Extract the module text.
    let contents = locator.slice(Range::new(
        Location::new(stmt.location.row(), 0),
        Location::new(stmt.end_location.unwrap().row() + 1, 0),
    ));

    // "Embed" it in a function definition, to preserve indentation while retaining valid source
    // code. (We'll strip the prefix later on.)
    let module_text = format!("def f():{}{contents}", stylist.line_ending().as_str());

    // Parse the CST.
    let mut tree = match_module(&module_text)?;

    // Extract the assert statement.
    let statements: &mut Vec<Statement> = {
        let [Statement::Compound(CompoundStatement::FunctionDef(embedding))] = &mut *tree.body else {
            bail!("Expected statement to be embedded in a function definition")
        };

        let Suite::IndentedBlock(indented_block) = &mut embedding.body else {
            bail!("Expected indented block")
        };
        indented_block.indent = Some(outer_indent);

        &mut indented_block.body
    };
    let [Statement::Simple(simple_statement_line)] = statements.as_mut_slice() else {
        bail!("Expected one simple statement")
    };
    let [SmallStatement::Assert(assert_statement)] = &mut *simple_statement_line.body else {
        bail!("Expected simple statement to be an assert")
    };

    if !(assert_statement.test.lpar().is_empty() && assert_statement.test.rpar().is_empty()) {
        bail!("Unable to split parenthesized condition");
    }

    // Extract the individual conditions.
    let mut conditions: Vec<Expression> = Vec::with_capacity(2);
    match &assert_statement.test {
        Expression::UnaryOperation(op) => {
            if matches!(op.operator, UnaryOp::Not { .. }) {
                if let Expression::BooleanOperation(op) = &*op.expression {
                    if matches!(op.operator, BooleanOp::Or { .. }) {
                        conditions.push(negate(&op.left));
                        conditions.push(negate(&op.right));
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
                conditions.push(*op.left.clone());
                conditions.push(*op.right.clone());
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

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    // Reconstruct and reformat the code.
    let module_text = state.to_string();
    let contents = module_text
        .strip_prefix(&format!("def f():{}", stylist.line_ending().as_str()))
        .unwrap()
        .to_string();

    Ok(Edit::replacement(
        contents,
        Location::new(stmt.location.row(), 0),
        Location::new(stmt.end_location.unwrap().row() + 1, 0),
    ))
}

/// PT018
pub fn composite_condition(checker: &mut Checker, stmt: &Stmt, test: &Expr, msg: Option<&Expr>) {
    let composite = is_composite_condition(test);
    if matches!(composite, CompositionKind::Simple | CompositionKind::Mixed) {
        let fixable = matches!(composite, CompositionKind::Simple)
            && msg.is_none()
            && !has_comments_in(Range::from(stmt), checker.locator);
        let mut diagnostic =
            Diagnostic::new(PytestCompositeAssertion { fixable }, Range::from(stmt));
        if fixable && checker.patch(diagnostic.kind.rule()) {
            diagnostic
                .try_set_fix(|| fix_composite_condition(stmt, checker.locator, checker.stylist));
        }
        checker.diagnostics.push(diagnostic);
    }
}
