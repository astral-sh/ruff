use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usage of the deprecated `task_concurrency` parameter in Airflow Operators.
///
/// ## Why is this bad?
/// The `task_concurrency` parameter has been deprecated and renamed to `max_active_tis_per_dag`
/// in Airflow 3.0. Code using the old parameter name needs to be updated to ensure
/// compatibility with Airflow 3.0.
///
/// ## Example
/// ```python
/// from airflow.operators.python import PythonOperator
///
/// # Invalid: using deprecated parameter
/// task = PythonOperator(task_id="my_task", task_concurrency=2)
/// ```
///
/// Use instead:
/// ```python
/// from airflow.operators.python import PythonOperator
///
/// # Valid: using new parameter name
/// task = PythonOperator(task_id="my_task", max_active_tis_per_dag=2)
/// ```
#[violation]
pub struct AirflowDeprecatedTaskConcurrency {
    pub old_param: String,
    pub new_param: String,
}

impl Violation for AirflowDeprecatedTaskConcurrency {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AirflowDeprecatedTaskConcurrency {
            old_param,
            new_param,
        } = self;
        format!("Use `{new_param}` instead of deprecated `{old_param}` parameter")
    }
}

/// AIR303
pub(crate) fn task_concurrency_check(checker: &mut Checker, value: &Expr) {
    // If the value is not a call, we can't do anything.
    let Expr::Call(ast::ExprCall {
        func: _, arguments, ..
    }) = value
    else {
        return;
    };

    // If we haven't seen any airflow imports, we can't do anything.
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // Check for the deprecated parameter
    if let Some(keyword) = arguments.find_keyword("task_concurrency") {
        checker.diagnostics.push(Diagnostic::new(
            AirflowDeprecatedTaskConcurrency {
                old_param: "task_concurrency".to_string(),
                new_param: "max_active_tis_per_dag".to_string(),
            },
            keyword.range(),
        ));
    }
}
