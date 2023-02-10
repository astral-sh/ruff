use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{
    Boolop, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword, Located, Stmt, StmtKind,
    Unaryop,
};

use super::helpers::is_falsy_constant;
use super::unittest_assert::UnittestAssert;
use crate::ast::helpers::{create_expr, create_stmt, unparse_stmt};
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violation::{AlwaysAutofixableViolation, AutofixKind, Availability, Violation};

define_violation!(
    /// ## What it does
    /// This violation is reported when the plugin encounter an assertion on multiple conditions.
    ///
    /// ## Why is this bad?
    /// Composite assertion statements are harder to understand and to debug when failures occur.
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
    pub struct CompositeAssertion;
);
impl Violation for CompositeAssertion {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|CompositeAssertion| format!("Break down assertion into multiple parts"))
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

/// Negate condition, i.e. `a` => `not a` and `not a` => `a`
pub fn negate(f: Located<ExprKind>) -> Located<ExprKind> {
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
pub fn fix_composite_condition(stylist: &Stylist, assert: &Stmt, test: &Expr) -> Option<Fix> {
    // We do not split compounds if there is a message
    if let StmtKind::Assert { msg: Some(_), .. } = &assert.node {
        return None;
    }

    let mut conditions: Vec<Located<ExprKind>> = vec![];
    match &test.node {
        ExprKind::BoolOp {
            op: Boolop::And,
            values,
        } => {
            // Compound, so split (Split)
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
                    // Only take cases without mixed `and` and `or`
                    if !values.iter().all(|mk| {
                        !matches!(
                            mk.node,
                            ExprKind::BoolOp {
                                op: Boolop::And,
                                ..
                            }
                        )
                    }) {
                        return None;
                    }

                    // `not (a or b)` equals `not a and not b`
                    let vals = values
                        .iter()
                        .map(|f| negate(f.clone()))
                        .collect::<Vec<Located<ExprKind>>>();

                    // Compound, so split (Split)
                    conditions.extend(vals);
                }
                _ => {
                    // Do not split
                    conditions.push(*operand.clone());
                }
            }
        }
        _ => {}
    };

    // for each condition create `assert condition`
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

    Some(Fix::replacement(
        content,
        assert.location,
        assert.end_location.unwrap(),
    ))
}

/// PT018
pub fn composite_condition(
    checker: &mut Checker,
    assert_stmt: &Stmt,
    test_expr: &Expr,
) -> Option<Diagnostic> {
    if is_composite_condition(test_expr) {
        let mut diagnostic = Diagnostic::new(CompositeAssertion, Range::from_located(assert_stmt));
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(fix) = fix_composite_condition(checker.stylist, assert_stmt, test_expr) {
                diagnostic.amend(fix);
            }
        }
        Some(diagnostic)
    } else {
        None
    }
}
