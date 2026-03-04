use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprCall, Stmt, StmtFunctionDef};
use ruff_python_semantic::{BindingKind, Modules, ScopeKind};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::is_airflow_task_variant;

/// ## What it does
/// Checks for branching logic that could be replaced with a short-circuit
/// pattern, either via `@task.branch` decorated functions or
/// `BranchPythonOperator` callables.
///
/// ## Why is this bad?
/// When a branch function has at least two `return` statements and exactly
/// one of them returns a non-empty list, the function is effectively acting
/// as a short-circuit operator. Using `@task.short_circuit` or
/// `ShortCircuitOperator` is simpler and more readable in such cases.
///
/// ## Example
///
/// Using the `TaskFlow` API:
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
///
/// Using the standard operator API:
/// ```python
/// from airflow.operators.python import BranchPythonOperator
///
///
/// def my_callable():
///     if condition:
///         return ["my_downstream_task"]
///     return []
///
///
/// task = BranchPythonOperator(task_id="my_task", python_callable=my_callable)
/// ```
///
/// Use instead:
/// ```python
/// from airflow.operators.python import ShortCircuitOperator
///
///
/// def my_callable():
///     return condition
///
///
/// task = ShortCircuitOperator(task_id="my_task", python_callable=my_callable)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct TaskBranchAsShortCircuit {
    kind: BranchKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchKind {
    Decorator,
    Operator,
}

impl Violation for TaskBranchAsShortCircuit {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            BranchKind::Decorator => {
                "A `@task.branch` that can be replaced with `@task.short_circuit`".to_string()
            }
            BranchKind::Operator => {
                "A `BranchPythonOperator` that can be replaced with `ShortCircuitOperator`"
                    .to_string()
            }
        }
    }
}

/// AIR003 (decorator form)
pub(crate) fn task_branch_as_short_circuit(checker: &Checker, function_def: &StmtFunctionDef) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    if !is_airflow_task_variant(function_def, checker.semantic(), "branch") {
        return;
    }

    if could_be_short_circuit(&function_def.body) {
        checker.report_diagnostic(
            TaskBranchAsShortCircuit {
                kind: BranchKind::Decorator,
            },
            function_def.range(),
        );
    }
}

/// AIR003 (operator form)
pub(crate) fn branch_python_operator_as_short_circuit(checker: &Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
        return;
    };

    if !matches!(
        qualified_name.segments(),
        [
            "airflow",
            "operators",
            "python" | "python_operator",
            "BranchPythonOperator"
        ] | [
            "airflow",
            "providers",
            "standard",
            "operators",
            "python",
            "BranchPythonOperator"
        ]
    ) {
        return;
    }

    let Some(keyword) = call.arguments.find_keyword("python_callable") else {
        return;
    };

    let Expr::Name(name_expr) = &keyword.value else {
        return;
    };

    let Some(binding_id) = semantic.only_binding(name_expr) else {
        return;
    };

    let BindingKind::FunctionDefinition(scope_id) = semantic.binding(binding_id).kind else {
        return;
    };

    let ScopeKind::Function(function_def) = semantic.scopes[scope_id].kind else {
        return;
    };

    if could_be_short_circuit(&function_def.body) {
        checker.report_diagnostic(
            TaskBranchAsShortCircuit {
                kind: BranchKind::Operator,
            },
            call.func.range(),
        );
    }
}

/// Returns `true` if the function body has 2+ return statements with exactly
/// one non-empty list return — indicating a short-circuit pattern.
fn could_be_short_circuit(body: &[Stmt]) -> bool {
    let mut visitor = ReturnStatementVisitor::default();
    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    let returns = &visitor.returns;
    if returns.len() < 2 {
        return false;
    }

    let non_empty_list_count = returns
        .iter()
        .filter(|ret| {
            ret.value.as_deref().is_some_and(
                |value| matches!(value, Expr::List(ast::ExprList { elts, .. }) if !elts.is_empty()),
            )
        })
        .count();

    non_empty_list_count == 1
}
