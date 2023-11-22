use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtIf};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type checks that do not raise `TypeError`.
///
/// ## Why is this bad?
/// The Python documentation states that `TypeError` should be raised upon
/// encountering an inappropriate type.
///
/// ## Example
/// ```python
/// def foo(n: int):
///     if isinstance(n, int):
///         pass
///     else:
///         raise ValueError("n must be an integer")
/// ```
///
/// Use instead:
/// ```python
/// def foo(n: int):
///     if isinstance(n, int):
///         pass
///     else:
///         raise TypeError("n must be an integer")
/// ```
///
/// ## References
/// - [Python documentation: `TypeError`](https://docs.python.org/3/library/exceptions.html#TypeError)
#[violation]
pub struct TypeCheckWithoutTypeError;

impl Violation for TypeCheckWithoutTypeError {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `TypeError` exception for invalid type")
    }
}

#[derive(Default)]
struct ControlFlowVisitor<'a> {
    returns: Vec<&'a Stmt>,
    breaks: Vec<&'a Stmt>,
    continues: Vec<&'a Stmt>,
}

impl<'a, 'b> StatementVisitor<'b> for ControlFlowVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                // Don't recurse.
            }
            Stmt::Return(_) => self.returns.push(stmt),
            Stmt::Break(_) => self.breaks.push(stmt),
            Stmt::Continue(_) => self.continues.push(stmt),
            _ => walk_stmt(self, stmt),
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
    checker
        .semantic()
        .resolve_call_path(call)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["", "isinstance" | "issubclass" | "callable"]
            )
        })
}

/// Returns `true` if an [`Expr`] is a test to check types (e.g. via isinstance)
fn check_type_check_test(checker: &mut Checker, test: &Expr) -> bool {
    match test {
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => values
            .iter()
            .all(|expr| check_type_check_test(checker, expr)),
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => check_type_check_test(checker, operand),
        Expr::Call(ast::ExprCall { func, .. }) => check_type_check_call(checker, func),
        _ => false,
    }
}

/// Returns `true` if `exc` is a reference to a builtin exception.
fn is_builtin_exception(checker: &mut Checker, exc: &Expr) -> bool {
    return checker
        .semantic()
        .resolve_call_path(exc)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                [
                    "",
                    "ArithmeticError"
                        | "AssertionError"
                        | "AttributeError"
                        | "BufferError"
                        | "EOFError"
                        | "Exception"
                        | "ImportError"
                        | "LookupError"
                        | "MemoryError"
                        | "NameError"
                        | "ReferenceError"
                        | "RuntimeError"
                        | "SyntaxError"
                        | "SystemError"
                        | "ValueError"
                ]
            )
        });
}

/// Returns `true` if an [`Expr`] is a reference to a builtin exception.
fn check_raise_type(checker: &mut Checker, exc: &Expr) -> bool {
    match exc {
        Expr::Name(_) => is_builtin_exception(checker, exc),
        Expr::Call(ast::ExprCall { func, .. }) => {
            if let Expr::Name(_) = func.as_ref() {
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
        checker
            .diagnostics
            .push(Diagnostic::new(TypeCheckWithoutTypeError, item.range()));
    }
}

/// Search the body of an if-condition for raises.
fn check_body(checker: &mut Checker, body: &[Stmt]) {
    for item in body {
        if has_control_flow(item) {
            return;
        }
        if let Stmt::Raise(ast::StmtRaise { exc: Some(exc), .. }) = &item {
            check_raise(checker, exc, item);
        }
    }
}

/// TRY004
pub(crate) fn type_check_without_type_error(
    checker: &mut Checker,
    stmt_if: &StmtIf,
    parent: Option<&Stmt>,
) {
    let StmtIf {
        body,
        test,
        elif_else_clauses,
        ..
    } = stmt_if;
    if let Some(Stmt::If(ast::StmtIf { test, .. })) = parent {
        if !check_type_check_test(checker, test) {
            return;
        }
    }

    // Only consider the body when the `if` condition is all type-related
    if !check_type_check_test(checker, test) {
        return;
    }
    check_body(checker, body);

    for clause in elif_else_clauses {
        if let Some(test) = &clause.test {
            // If there are any `elif`, they must all also be type-related
            if !check_type_check_test(checker, test) {
                return;
            }
        }

        // The `elif` or `else` body raises the wrong exception
        check_body(checker, &clause.body);
    }
}
