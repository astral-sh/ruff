use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{
    Boolop, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword, Stmt, StmtKind, Unaryop,
};

use super::helpers::is_falsy_constant;
use super::unittest_assert::UnittestAssert;
use crate::ast::helpers::unparse_stmt;
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{AlwaysAutofixableViolation, Violation};

define_violation!(
    pub struct CompositeAssertion;
);
impl Violation for CompositeAssertion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
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
            "Found assertion on exception `{name}` in except block, use `pytest.raises()` instead"
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
    }
);
impl AlwaysAutofixableViolation for UnittestAssertion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnittestAssertion { assertion } = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn autofix_title(&self) -> String {
        let UnittestAssertion { assertion } = self;
        format!("Replace `{assertion}(...)` with `assert ...`")
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

/// Check if the test expression is a composite condition.
/// For example, `a and b` or `not (a or b)`. The latter is equivalent
/// to `not a and not b` by De Morgan's laws.
const fn is_composite_condition(test: &Expr) -> bool {
    match &test.node {
        ExprKind::BoolOp {
            op: Boolop::And, ..
        } => true,
        ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } => matches!(&operand.node, ExprKind::BoolOp { op: Boolop::Or, .. }),
        _ => false,
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
                let mut diagnostic = Diagnostic::new(
                    UnittestAssertion {
                        assertion: unittest_assert.to_string(),
                    },
                    Range::from_located(func),
                );
                if checker.patch(diagnostic.kind.rule()) {
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
pub fn assert_falsy(assert_stmt: &Stmt, test_expr: &Expr) -> Option<Diagnostic> {
    if is_falsy_constant(test_expr) {
        Some(Diagnostic::new(
            AssertAlwaysFalse,
            Range::from_located(assert_stmt),
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

/// PT018
pub fn composite_condition(assert_stmt: &Stmt, test_expr: &Expr) -> Option<Diagnostic> {
    if is_composite_condition(test_expr) {
        Some(Diagnostic::new(
            CompositeAssertion,
            Range::from_located(assert_stmt),
        ))
    } else {
        None
    }
}
