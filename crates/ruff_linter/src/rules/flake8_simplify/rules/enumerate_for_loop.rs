use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ExceptHandler, Expr, Int, MatchCase, Number, Operator, Stmt};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops with explicit loop-index variables that can be replaced
/// with `enumerate()`.
///
/// ## Why this is bad?
/// When iterating over a sequence, it's often desirable to keep track of the
/// index of each element alongside the element itself. Prefer the `enumerate`
/// builtin over manually incrementing a counter variable within the loop, as
/// `enumerate` is more concise and idiomatic.
///
/// ## Example
/// ```python
/// fruits = ["apple", "banana", "cherry"]
/// for fruit in fruits:
///     print(f"{i + 1}. {fruit}")
///     i += 1
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["apple", "banana", "cherry"]
/// for i, fruit in enumerate(fruits):
///     print(f"{i + 1}. {fruit}")
/// ```
///
/// ## References
/// - [Python documentation: `enumerate`](https://docs.python.org/3/library/functions.html#enumerate)
#[violation]
pub struct EnumerateForLoop {
    index: String,
}

impl Violation for EnumerateForLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EnumerateForLoop { index } = self;
        format!("Use `enumerate()` for index variable `{index}` in `for` loop")
    }
}

/// SIM113
pub(crate) fn enumerate_for_loop(checker: &mut Checker, for_stmt: &ast::StmtFor) {
    // If the loop contains a `continue`, abort.
    if has_continue(&for_stmt.body) {
        return;
    };

    // If the loop contains an increment statement (e.g., `i += 1`)...
    for stmt in &for_stmt.body {
        if let Some(index) = match_index_increment(stmt) {
            // Find the binding corresponding to the augmented assignment (e.g., `i += 1`).
            let Some(id) = checker.semantic().resolve_name(index) else {
                continue;
            };
            let binding = checker.semantic().binding(id);

            // If it's not an assignment (e.g., it's a function argument), ignore it.
            if !binding.kind.is_assignment() {
                continue;
            }

            // Ensure that the index variable was initialized to 0.
            let Some(value) = typing::find_binding_value(&index.id, binding, checker.semantic())
            else {
                continue;
            };
            let Expr::NumberLiteral(ast::ExprNumberLiteral { value: num, .. }) = value else {
                continue;
            };
            let Some(int) = num.as_int() else {
                continue;
            };
            if *int != Int::ZERO {
                continue;
            }

            // If the binding is not at the same level as the `for` loop (e.g., it's in an `if`),
            // ignore it.
            let Some(for_loop_id) = checker.semantic().current_statement_id() else {
                continue;
            };
            let Some(assignment_id) = binding.source else {
                continue;
            };
            if !checker.semantic().same_branch(for_loop_id, assignment_id) {
                continue;
            }

            // If there are multiple assignments to this variable _within_ the loop, ignore it.
            if checker
                .semantic()
                .current_scope()
                .get_all(&index.id)
                .map(|id| checker.semantic().binding(id))
                .filter(|binding| for_stmt.range().contains_range(binding.range()))
                .count()
                > 1
            {
                continue;
            }

            let diagnostic = Diagnostic::new(
                EnumerateForLoop {
                    index: index.id.to_string(),
                },
                stmt.range(),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Recursively check if the `for` loop body contains a `continue` statement
fn has_continue(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Continue(_) => true,
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            has_continue(body)
                || elif_else_clauses
                    .iter()
                    .any(|clause| has_continue(&clause.body))
        }
        Stmt::With(ast::StmtWith { body, .. }) => has_continue(body),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .any(|MatchCase { body, .. }| has_continue(body)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            has_continue(body)
                || has_continue(orelse)
                || has_continue(finalbody)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => has_continue(body),
                })
        }

        _ => false,
    })
}

/// If the statement is an index increment statement (e.g., `i += 1`), return
/// the name of the index variable.
fn match_index_increment(stmt: &Stmt) -> Option<&ast::ExprName> {
    let Stmt::AugAssign(ast::StmtAugAssign {
        target,
        op: Operator::Add,
        value,
        ..
    }) = stmt
    else {
        return None;
    };

    let name = target.as_name_expr()?;

    if let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: Number::Int(value),
        ..
    }) = value.as_ref()
    {
        if matches!(*value, Int::ONE) {
            return Some(name);
        }
    }

    None
}
