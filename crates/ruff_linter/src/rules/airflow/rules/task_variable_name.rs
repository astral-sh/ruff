use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that the task variable name matches the `task_id` value for
/// Airflow Operators.
///
/// ## Why is this bad?
/// When initializing an Airflow Operator, for consistency, the variable
/// name should match the `task_id` value. This makes it easier to
/// follow the flow of the DAG.
///
/// ## Example
/// ```python
/// from airflow.operators import PythonOperator
///
///
/// incorrect_name = PythonOperator(task_id="my_task")
/// ```
///
/// Use instead:
/// ```python
/// from airflow.operators import PythonOperator
///
///
/// my_task = PythonOperator(task_id="my_task")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AirflowVariableNameTaskIdMismatch {
    task_id: String,
}

impl Violation for AirflowVariableNameTaskIdMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AirflowVariableNameTaskIdMismatch { task_id } = self;
        format!("Task variable name should match the `task_id`: \"{task_id}\"")
    }
}

/// AIR001
pub(crate) fn variable_name_task_id(checker: &Checker, targets: &[Expr], value: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // If we have more than one target, we can't do anything.
    let [target] = targets else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    // If the value is not a call, we can't do anything.
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = value
    else {
        return;
    };

    // If the function doesn't come from Airflow's operators module (builtin or providers), we
    // can't do anything.
    if !checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            match qualified_name.segments() {
                // Match `airflow.operators.*`
                ["airflow", "operators", ..] => true,

                // Match `airflow.providers.**.operators.*`
                ["airflow", "providers", rest @ ..] => {
                    // Ensure 'operators' exists somewhere in the middle
                    if let Some(pos) = rest.iter().position(|&s| s == "operators") {
                        pos + 1 < rest.len() // Check that 'operators' is not the last element
                    } else {
                        false
                    }
                }

                _ => false,
            }
        })
    {
        return;
    }

    // If the call doesn't have a `task_id` keyword argument, we can't do anything.
    let Some(keyword) = arguments.find_keyword("task_id") else {
        return;
    };

    // If the keyword argument is not a string, we can't do anything.
    let Some(ast::ExprStringLiteral { value: task_id, .. }) =
        keyword.value.as_string_literal_expr()
    else {
        return;
    };

    // If the target name is the same as the task_id, no violation.
    if task_id == id.as_str() {
        return;
    }

    let diagnostic = Diagnostic::new(
        AirflowVariableNameTaskIdMismatch {
            task_id: task_id.to_string(),
        },
        target.range(),
    );
    checker.report_diagnostic(diagnostic);
}
