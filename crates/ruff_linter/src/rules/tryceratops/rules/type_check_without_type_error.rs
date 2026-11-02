use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{Expr, ExprKind, Stmt, StmtIf, StmtKind, Suite};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.230", category = Category::Style)]
pub(crate) struct TypeCheckWithoutTypeError;

impl Violation for TypeCheckWithoutTypeError {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Prefer `TypeError` exception for invalid type".to_string()
    }
}

#[derive(Default)]
struct ControlFlowVisitor<'a> {
    returns: Vec<Stmt<'a>>,
    breaks: Vec<Stmt<'a>>,
    continues: Vec<Stmt<'a>>,
}

impl<'a> StatementVisitor<'a> for ControlFlowVisitor<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        match stmt.classify() {
            StmtKind::FunctionDef(_) | StmtKind::ClassDef(_) => {
                // Don't recurse.
            }
            StmtKind::Return(_) => self.returns.push(stmt),
            StmtKind::Break(_) => self.breaks.push(stmt),
            StmtKind::Continue(_) => self.continues.push(stmt),
            _ => walk_stmt(self, stmt),
        }
    }
}

/// Returns `true` if a [`Stmt`] contains a `return`, `break`, or `continue`.
fn has_control_flow(stmt: Stmt<'_>) -> bool {
    let mut visitor = ControlFlowVisitor::default();
    visitor.visit_stmt(stmt);
    !visitor.returns.is_empty() || !visitor.breaks.is_empty() || !visitor.continues.is_empty()
}

/// Returns `true` if an [`Expr`] is a call to check types.
fn check_type_check_call(semantic: &SemanticModel, call: Expr<'_>) -> bool {
    semantic
        .resolve_builtin_symbol(call)
        .is_some_and(|builtin| matches!(builtin, "isinstance" | "issubclass" | "callable"))
}

/// Returns `true` if an [`Expr`] is a test to check types (e.g. via isinstance)
fn check_type_check_test(semantic: &SemanticModel, test: Expr<'_>) -> bool {
    match test.classify() {
        ExprKind::BoolOp(expr_bool_op) => {
            let values = expr_bool_op.values();
            values
                .iter()
                .all(|expr| check_type_check_test(semantic, expr))
        }
        ExprKind::UnaryOp(expr_unary_op) => {
            let operand = expr_unary_op.operand();

            check_type_check_test(semantic, operand)
        }
        ExprKind::Call(expr_call) => {
            let func = expr_call.func();

            check_type_check_call(semantic, func)
        }
        _ => false,
    }
}

fn check_raise(checker: &Checker, exc: Expr<'_>, item: Stmt<'_>) {
    if is_builtin_exception(exc, checker.semantic()) {
        checker.report_diagnostic(TypeCheckWithoutTypeError, item.range());
    }
}

/// Search the body of an if-condition for raises.
fn check_body(checker: &Checker, body: Suite<'_>) {
    for item in body {
        if has_control_flow(item) {
            return;
        }
        if let Some(raise) = item.as_raise_stmt()
            && let Some(exc) = raise.exc()
        {
            check_raise(checker, exc, item);
        }
    }
}

/// Returns `true` if the given expression is a builtin exception.
///
/// This function only matches to a subset of the builtin exceptions, and omits `TypeError`.
fn is_builtin_exception(expr: Expr<'_>, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(map_callable(expr))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
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
        })
}

/// TRY004
pub(crate) fn type_check_without_type_error(
    checker: &Checker,
    stmt_if: StmtIf<'_>,
    parent: Option<Stmt<'_>>,
) {
    let body = stmt_if.body();
    let test = stmt_if.test();
    let elif_else_clauses = stmt_if.elif_else_clauses();

    if let Some(parent) = parent.and_then(ruff_python_ast::Stmt::as_if_stmt)
        && !check_type_check_test(checker.semantic(), parent.test())
    {
        return;
    }

    // Only consider the body when the `if` condition is all type-related
    if !check_type_check_test(checker.semantic(), test) {
        return;
    }
    check_body(checker, body);

    for clause in elif_else_clauses {
        if let Some(test) = clause.test() {
            // If there are any `elif`, they must all also be type-related
            if !check_type_check_test(checker.semantic(), test) {
                return;
            }
        }

        // The `elif` or `else` body raises the wrong exception
        check_body(checker, clause.body());
    }
}
