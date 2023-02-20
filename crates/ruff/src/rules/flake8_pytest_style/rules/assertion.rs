use rustpython_parser::ast::{
    Boolop, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword, Stmt, StmtKind, Unaryop,
};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::{create_expr, create_stmt, unparse_stmt};
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violation::{AutofixKind, Availability, Violation};

use super::helpers::is_falsy_constant;
use super::unittest_assert::UnittestAssert;

define_violation!(
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
    /// def test_bar():
    ///     assert not something
    ///     assert not something_else
    /// ```
    pub struct CompositeAssertion {
        pub fixable: bool,
    }
);
impl Violation for CompositeAssertion {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let CompositeAssertion { fixable } = self;
        if *fixable {
            Some(|_| format!("Break down assertion into multiple parts"))
        } else {
            None
        }
    }
}

define_violation!(
    pub struct AssertInExcept {
        pub name: String,
    }
);
impl Violation for AssertInExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssertInExcept { name } = self;
        format!(
            "Found assertion on exception `{name}` in `except` block, use `pytest.raises()` instead"
        )
    }
}

define_violation!(
    pub struct AssertAlwaysFalse;
);
impl Violation for AssertAlwaysFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion always fails, replace with `pytest.fail()`")
    }
}

define_violation!(
    pub struct UnittestAssertion {
        pub assertion: String,
        pub fixable: bool,
    }
);
impl Violation for UnittestAssertion {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnittestAssertion { assertion, .. } = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|UnittestAssertion { assertion, .. }| {
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
                            AssertInExcept {
                                name: id.to_string(),
                            },
                            Range::from_located(current_assert),
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
    call: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    match &func.node {
        ExprKind::Attribute { attr, .. } => {
            if let Ok(unittest_assert) = UnittestAssert::try_from(attr.as_str()) {
                // We're converting an expression to a statement, so avoid applying the fix if
                // the assertion is part of a larger expression.
                let fixable = checker.current_expr_parent().is_none()
                    && matches!(checker.current_stmt().node, StmtKind::Expr { .. });
                let mut diagnostic = Diagnostic::new(
                    UnittestAssertion {
                        assertion: unittest_assert.to_string(),
                        fixable,
                    },
                    Range::from_located(func),
                );
                if fixable && checker.patch(diagnostic.kind.rule()) {
                    if let Ok(stmt) = unittest_assert.generate_assert(args, keywords) {
                        diagnostic.amend(Fix::replacement(
                            unparse_stmt(&stmt, checker.stylist),
                            call.location,
                            call.end_location.unwrap(),
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
        Some(Diagnostic::new(
            AssertAlwaysFalse,
            Range::from_located(stmt),
        ))
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
fn negate(f: Expr) -> Expr {
    match f.node {
        ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } => *operand,
        _ => create_expr(ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand: Box::new(f),
        }),
    }
}

/// Replace composite condition `assert a == "hello" and b == "world"` with two statements
/// `assert a == "hello"` and `assert b == "world"`.
fn fix_composite_condition(stylist: &Stylist, stmt: &Stmt, test: &Expr) -> Fix {
    let mut conditions: Vec<Expr> = vec![];
    match &test.node {
        ExprKind::BoolOp {
            op: Boolop::And,
            values,
        } => {
            // Compound, so split.
            conditions.extend(values.clone());
        }
        ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } => {
            match &operand.node {
                ExprKind::BoolOp {
                    op: Boolop::Or,
                    values,
                } => {
                    // Split via `not (a or b)` equals `not a and not b`.
                    conditions.extend(values.iter().map(|f| negate(f.clone())));
                }
                _ => {
                    // Do not split.
                    conditions.push(*operand.clone());
                }
            }
        }
        _ => {}
    };

    // For each condition, create an `assert condition` statement.
    let mut content: Vec<String> = Vec::with_capacity(conditions.len());
    for condition in conditions {
        content.push(unparse_stmt(
            &create_stmt(StmtKind::Assert {
                test: Box::new(condition.clone()),
                msg: None,
            }),
            stylist,
        ));
    }

    let content = content.join(stylist.line_ending().as_str());
    Fix::replacement(content, stmt.location, stmt.end_location.unwrap())
}

/// PT018
pub fn composite_condition(checker: &mut Checker, stmt: &Stmt, test: &Expr, msg: Option<&Expr>) {
    let composite = is_composite_condition(test);
    if matches!(composite, CompositionKind::Simple | CompositionKind::Mixed) {
        let fixable = matches!(composite, CompositionKind::Simple) && msg.is_none();
        let mut diagnostic =
            Diagnostic::new(CompositeAssertion { fixable }, Range::from_located(stmt));
        if fixable && checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(fix_composite_condition(checker.stylist, stmt, test));
        }
        checker.diagnostics.push(diagnostic);
    }
}
