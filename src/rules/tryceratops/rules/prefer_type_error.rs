use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PreferTypeError;
);
impl Violation for PreferTypeError {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `TypeError` exception for invalid type")
    }
}

// check if a call is checking types
fn check_type_check(checker: &mut Checker, call: &Expr) -> bool {
    let call_path = checker.resolve_call_path(call);
    let ids = vec!["isinstance", "issubclass", "callable"];
    return call_path.as_ref().map_or(false, |call_path| {
        ids.iter().any(|&i| *call_path.as_slice() == ["", i])
    });
}

// check if the test of an If statements is checking types (e.g. via isinstance)
fn check_test(checker: &mut Checker, test: &Expr) -> bool {
    match &test.node {
        ExprKind::BoolOp { values, .. } => {
            return values.iter().all(|e| check_test(checker, e));
        }
        ExprKind::UnaryOp { operand, .. } => check_test(checker, operand),
        ExprKind::Call { .. } => check_type_check(checker, test),
        ExprKind::Compare { .. } => false,
        _ => false,
    }
}

fn get_name(exc: &Expr) -> Option<&Expr> {
    match &exc.node {
        ExprKind::Name { .. } => Some(exc),
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { .. } = &func.node {
                return Some(func);
            }
            None
        }
        _ => None,
    }
}

fn check_raise_type(exc: &Expr) -> bool {
    let builtin_exceptions = vec![
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
    ];

    match &exc.node {
        ExprKind::Name { id, .. } => {
            return builtin_exceptions.contains(&id.as_str());
        }
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { id, .. } = &func.node {
                return builtin_exceptions.contains(&id.as_str());
            }
            false
        }
        _ => false,
    }
}

fn check_raise(checker: &mut Checker, exc: &Expr, item: &Stmt) {
    if check_raise_type(exc) {
        let mut diagnostic = Diagnostic::new(PreferTypeError, Range::from_located(item));

        if checker.patch(diagnostic.kind.rule()) {
            if let Some(name) = get_name(exc) {
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

// Search body of if-condition for raises
fn check_body(checker: &mut Checker, func: &Vec<Stmt>) {
    for item in func {
        if let StmtKind::Raise { exc: Some(exc), .. } = &item.node {
            check_raise(checker, exc, item);
        }
    }
}

fn check_orelse(checker: &mut Checker, func: &Vec<Stmt>) {
    for item in func {
        match &item.node {
            StmtKind::If { test, .. } => {
                if !check_test(checker, test) {
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
    body: &Vec<Stmt>,
    test: &Expr,
    orelse: &Vec<Stmt>,
    parent: Option<&Stmt>,
) {
    if let Some(parent) = parent {
        if let StmtKind::If { test, .. } = &parent.node {
            if !check_test(checker, test) {
                return;
            }
        }
    }

    // Only consider the body when the `if` condition is all type-related
    if check_test(checker, test) {
        check_body(checker, body);
        check_orelse(checker, orelse);
    }
}
