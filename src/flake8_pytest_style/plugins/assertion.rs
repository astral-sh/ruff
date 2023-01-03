use rustpython_ast::{
    Boolop, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind, Unaryop,
};

use super::helpers::is_falsy_constant;
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::registry::{Check, CheckKind};

/// Visitor that tracks assert statements and checks if they reference
/// the exception name.
struct ExceptionHandlerVisitor<'a> {
    exception_name: &'a str,
    current_assert: Option<&'a Stmt>,
    errors: Vec<Check>,
}

impl<'a> ExceptionHandlerVisitor<'a> {
    fn new(exception_name: &'a str) -> Self {
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
                        self.errors.push(Check::new(
                            CheckKind::AssertInExcept(id.to_string()),
                            Range::from_located(current_assert),
                        ));
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

const UNITTEST_ASSERT_NAMES: &[&str] = &[
    "assertAlmostEqual",
    "assertAlmostEquals",
    "assertDictEqual",
    "assertEqual",
    "assertEquals",
    "assertFalse",
    "assertGreater",
    "assertGreaterEqual",
    "assertIn",
    "assertIs",
    "assertIsInstance",
    "assertIsNone",
    "assertIsNot",
    "assertIsNotNone",
    "assertItemsEqual",
    "assertLess",
    "assertLessEqual",
    "assertMultiLineEqual",
    "assertNotAlmostEqual",
    "assertNotAlmostEquals",
    "assertNotContains",
    "assertNotEqual",
    "assertNotEquals",
    "assertNotIn",
    "assertNotIsInstance",
    "assertNotRegexpMatches",
    "assertRaises",
    "assertRaisesMessage",
    "assertRaisesRegexp",
    "assertRegexpMatches",
    "assertSetEqual",
    "assertTrue",
    "assert_",
];

/// Check if the test expression is a composite condition.
/// For example, `a and b` or `not (a or b)`. The latter is equivalent
/// to `not a and not b` by De Morgan's laws.
fn is_composite_condition(test: &Expr) -> bool {
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

fn check_assert_in_except(name: &str, body: &[Stmt]) -> Vec<Check> {
    // Walk body to find assert statements that reference the exception name
    let mut visitor = ExceptionHandlerVisitor::new(name);
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
    visitor.errors
}

/// PT009
pub fn unittest_assertion(call: &Expr) -> Option<Check> {
    match &call.node {
        ExprKind::Attribute { attr, .. } => {
            if UNITTEST_ASSERT_NAMES.contains(&attr.as_str()) {
                Some(Check::new(
                    CheckKind::UnittestAssertion(attr.to_string()),
                    Range::from_located(call),
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// PT015
pub fn assert_falsy(assert_stmt: &Stmt, test_expr: &Expr) -> Option<Check> {
    if is_falsy_constant(test_expr) {
        Some(Check::new(
            CheckKind::AssertAlwaysFalse,
            Range::from_located(assert_stmt),
        ))
    } else {
        None
    }
}

/// PT017
pub fn assert_in_exception_handler(handlers: &[Excepthandler]) -> Vec<Check> {
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
pub fn composite_condition(assert_stmt: &Stmt, test_expr: &Expr) -> Option<Check> {
    if is_composite_condition(test_expr) {
        Some(Check::new(
            CheckKind::CompositeAssertion,
            Range::from_located(assert_stmt),
        ))
    } else {
        None
    }
}
