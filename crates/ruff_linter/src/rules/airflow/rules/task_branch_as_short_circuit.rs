use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{ReturnStatementVisitor, map_callable};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, StmtFunctionDef};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `@task.branch` decorated functions that could be replaced
/// with `@task.short_circuit`.
///
/// ## Why is this bad?
/// When a `@task.branch` function has at least two `return` statements and
/// exactly one of them returns a non-empty list, the function is effectively
/// acting as a short-circuit operator. Using `@task.short_circuit` is
/// simpler and more readable in such cases.
///
/// ## Example
/// ```python
/// from airflow.decorators import task
///
///
/// @task.branch
/// def my_task():
///     if condition:
///         return ["my_downstream_task"]
///     return []
/// ```
///
/// Use instead:
/// ```python
/// from airflow.decorators import task
///
///
/// @task.short_circuit
/// def my_task():
///     return condition
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct TaskBranchAsShortCircuit;

impl Violation for TaskBranchAsShortCircuit {
    #[derive_message_formats]
    fn message(&self) -> String {
        "A `@task.branch` that can be replaced with `@task.short_circuit`".to_string()
    }
}

/// AIR003
pub(crate) fn task_branch_as_short_circuit(checker: &Checker, function_def: &StmtFunctionDef) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    if !has_task_branch_decorator(function_def, checker) {
        return;
    }

    let mut visitor = ReturnStatementVisitor::default();
    for stmt in &function_def.body {
        visitor.visit_stmt(stmt);
    }

    let returns = &visitor.returns;
    if returns.len() < 2 {
        return;
    }

    let non_empty_list_count = returns
        .iter()
        .filter(|ret| {
            ret.value.as_deref().is_some_and(
                |value| matches!(value, Expr::List(ast::ExprList { elts, .. }) if !elts.is_empty()),
            )
        })
        .count();

    if non_empty_list_count == 1 {
        checker.report_diagnostic(TaskBranchAsShortCircuit, function_def.range());
    }
}

/// Returns `true` if the function is decorated with `@task.branch`.
fn has_task_branch_decorator(function_def: &StmtFunctionDef, checker: &Checker) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        let expr = map_callable(&decorator.expression);
        if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = expr {
            if attr.as_str() == "branch" {
                return checker
                    .semantic()
                    .resolve_qualified_name(value)
                    .is_some_and(|qualified_name| {
                        matches!(qualified_name.segments(), ["airflow", "decorators", "task"])
                    });
            }
        }
        false
    })
}
