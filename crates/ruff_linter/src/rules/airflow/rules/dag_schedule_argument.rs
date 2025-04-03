use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Expr;
use ruff_python_ast::{self as ast};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for a `DAG()` class or `@dag()` decorator without an explicit
/// `schedule` (or `schedule_interval` for Airflow 1) parameter.
///
/// ## Why is this bad?
/// The default value of the `schedule` parameter on Airflow 2 and
/// `schedule_interval` on Airflow 1 is `timedelta(days=1)`, which is almost
/// never what a user is looking for. Airflow 3 changed the default value to `None`,
/// and would break existing dags using the implicit default.
///
/// If your dag does not have an explicit `schedule` / `schedule_interval` argument,
/// Airflow 2 schedules a run for it every day (at the time determined by `start_date`).
/// Such a dag will no longer be scheduled on Airflow 3 at all, without any
/// exceptions or other messages visible to the user.
///
/// ## Example
/// ```python
/// from airflow import DAG
///
///
/// # Using the implicit default schedule.
/// dag = DAG(dag_id="my_dag")
/// ```
///
/// Use instead:
/// ```python
/// from datetime import timedelta
///
/// from airflow import DAG
///
///
/// dag = DAG(dag_id="my_dag", schedule=timedelta(days=1))
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AirflowDagNoScheduleArgument;

impl Violation for AirflowDagNoScheduleArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        "DAG should have an explicit `schedule` argument".to_string()
    }
}

/// AIR002
pub(crate) fn dag_no_schedule_argument(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // Don't check non-call expressions.
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return;
    };

    // We don't do anything unless this is a `DAG` (class) or `dag` (decorator
    // function) from Airflow.
    if !checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualname| matches!(qualname.segments(), ["airflow", .., "DAG" | "dag"]))
    {
        return;
    }

    // If there's a schedule keyword argument, we are good.
    // This includes the canonical 'schedule', and the deprecated 'timetable'
    // and 'schedule_interval'. Usages of deprecated schedule arguments are
    // covered by AIR301.
    if ["schedule", "schedule_interval", "timetable"]
        .iter()
        .any(|a| arguments.find_keyword(a).is_some())
    {
        return;
    }

    // Produce a diagnostic when the `schedule` keyword argument is not found.
    let diagnostic = Diagnostic::new(AirflowDagNoScheduleArgument, expr.range());
    checker.report_diagnostic(diagnostic);
}
