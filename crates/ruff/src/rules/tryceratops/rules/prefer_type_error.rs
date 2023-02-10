use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct PreferTypeError;
);
impl AlwaysAutofixableViolation for PreferTypeError {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `TypeError` exception for invalid type")
    }

    fn autofix_title(&self) -> String {
        "Use `TypeError` exception type".to_string()
    }
}

#[derive(Default)]
struct ControlFlowVisitor<'a> {
    returns: Vec<&'a Stmt>,
    breaks: Vec<&'a Stmt>,
    continues: Vec<&'a Stmt>,
}

impl<'a, 'b> Visitor<'b> for ControlFlowVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                // Don't recurse.
            }
            StmtKind::Return { .. } => self.returns.push(stmt),
            StmtKind::Break => self.breaks.push(stmt),
            StmtKind::Continue => self.continues.push(stmt),
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. }
            | ExprKind::GeneratorExp { .. } => {
                // Don't recurse.
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// Returns `true` if a [`Stmt`] contains a `return`, `break`, or `continue`.
fn has_control_flow(stmt: &Stmt) -> bool {
    let mut visitor = ControlFlowVisitor::default();
    visitor.visit_stmt(stmt);
    !visitor.returns.is_empty() || !visitor.breaks.is_empty() || !visitor.continues.is_empty()
}

/// Returns `true` if an [`Expr`] is a call to check types.
fn check_type_check_call(checker: &mut Checker, call: &Expr) -> bool {
    checker.resolve_call_path(call).map_or(false, |call_path| {
        call_path.as_slice() == ["", "isinstance"]
            || call_path.as_slice() == ["", "issubclass"]
            || call_path.as_slice() == ["", "callable"]
    })
}

/// Returns `true` if an [`Expr`] is a test to check types (e.g. via isinstance)
fn check_type_check_test(checker: &mut Checker, test: &Expr) -> bool {
    match &test.node {
        ExprKind::BoolOp { values, .. } => values
            .iter()
            .all(|expr| check_type_check_test(checker, expr)),
        ExprKind::UnaryOp { operand, .. } => check_type_check_test(checker, operand),
        ExprKind::Call { func, .. } => check_type_check_call(checker, func),
        _ => false,
    }
}

/// Returns the [`Expr`] representing the name of the exception.
const fn match_name(exc: &Expr) -> Option<&Expr> {
    match &exc.node {
        ExprKind::Name { .. } => Some(exc),
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { .. } = &func.node {
                Some(func)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Returns `true` if `exc` is a reference to a builtin exception.
fn is_builtin_exception(checker: &mut Checker, exc: &Expr) -> bool {
    return checker.resolve_call_path(exc).map_or(false, |call_path| {
        [
            "ArithmeticError",
            "AssertionError",
            "AttributeError",
            "BufferError",
            "EOFError",
            "Exception",
            "ImportError",
            "LookupError",
            "MemoryError",
            "NameError",
            "ReferenceError",
            "RuntimeError",
            "SyntaxError",
            "SystemError",
            "ValueError",
        ]
        .iter()
        .any(|target| call_path.as_slice() == ["", target])
    });
}

/// Returns `true` if an [`Expr`] is a reference to a builtin exception.
fn check_raise_type(checker: &mut Checker, exc: &Expr) -> bool {
    match &exc.node {
        ExprKind::Name { .. } => is_builtin_exception(checker, exc),
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { .. } = &func.node {
                is_builtin_exception(checker, func)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn check_raise(checker: &mut Checker, exc: &Expr, item: &Stmt) {
    if check_raise_type(checker, exc) {
        let mut diagnostic = Diagnostic::new(PreferTypeError, Range::from_located(item));

        if checker.patch(diagnostic.kind.rule()) {
            if let Some(name) = match_name(exc) {
                diagnostic.amend(Fix::replacement(
                    "TypeError".to_string(),
                    name.location,
                    name.end_location.unwrap(),
                ));
            }
        }

        checker.diagnostics.push(diagnostic);
    }
}

/// Search the body of an if-condition for raises.
fn check_body(checker: &mut Checker, body: &[Stmt]) {
    for item in body {
        if has_control_flow(item) {
            return;
        }
        if let StmtKind::Raise { exc: Some(exc), .. } = &item.node {
            check_raise(checker, exc, item);
        }
    }
}

/// Search the orelse of an if-condition for raises.
fn check_orelse(checker: &mut Checker, body: &[Stmt]) {
    for item in body {
        if has_control_flow(item) {
            return;
        }
        match &item.node {
            StmtKind::If { test, .. } => {
                if !check_type_check_test(checker, test) {
                    return;
                }
            }
            StmtKind::Raise { exc: Some(exc), .. } => {
                check_raise(checker, exc, item);
            }
            _ => {}
        }
    }
}

/// TRY004
pub fn prefer_type_error(
    checker: &mut Checker,
    body: &[Stmt],
    test: &Expr,
    orelse: &[Stmt],
    parent: Option<&Stmt>,
) {
    if let Some(parent) = parent {
        if let StmtKind::If { test, .. } = &parent.node {
            if !check_type_check_test(checker, test) {
                return;
            }
        }
    }

    // Only consider the body when the `if` condition is all type-related
    if check_type_check_test(checker, test) {
        check_body(checker, body);
        check_orelse(checker, orelse);
    }
}
