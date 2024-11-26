use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_ast::{self as ast, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for a `DAG()` class or `@dag()` decorator without an explicit
/// `schedule` parameter.
///
/// ## Why is this bad?
/// The default `schedule` value on Airflow 2 is `timedelta(days=1)`, which is
/// almost never what a user is looking for. Airflow 3 changes this the default
/// to *None*, and would break existing DAGs using the implicit default.
///
/// If your DAG does not have an explicit `schedule` argument, Airflow 2
/// schedules a run for it every day (at the time determined by `start_date`).
/// Such a DAG will no longer be scheduled on Airflow 3 at all, without any
/// exceptions or other messages visible to the user.
///
/// Airflow 2 also provides alternative arguments `schedule_interval` and
/// `timetable` to specify the DAG schedule. They existed for backward
/// compatibility, and have been removed from Airflow 3.
///
/// ## Example
/// ```python
/// from airflow import DAG
///
///
/// # Using the implicit default schedule.
/// dag1 = DAG(dag_id="my_dag_1")
///
/// # Using a deprecated argument to set schedule.
/// dag2 = DAG(dag_id="my_dag_2", schedule_interval="@daily")
/// ```
///
/// Use instead:
/// ```python
/// from datetime import timedelta
///
/// from airflow import DAG
///
///
/// dag1 = DAG(dag_id="my_dag_1", schedule=timedelta(days=1))
/// dag2 = DAG(dag_id="my_dag_2", schedule="@daily")
/// ```
#[violation]
pub struct AirflowDagNoScheduleArgument {
    deprecated_argument: Option<String>,
}

impl Violation for AirflowDagNoScheduleArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AirflowDagNoScheduleArgument {
            deprecated_argument,
        } = self;
        match deprecated_argument {
            Some(argument) => {
                format!("argument `{argument}` is deprecated; use `schedule` instead")
            }
            None => "DAG should have an explicit `schedule` argument".to_string(),
        }
    }
}

/// AIR301
pub(crate) fn dag_no_schedule_argument(checker: &mut Checker, expr: &Expr) {
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

    // If there's a `schedule` keyword argument, we are good.
    if arguments.find_keyword("schedule").is_some() {
        return;
    }

    // Produce a diagnostic on either a deprecated schedule keyword argument,
    // or no schedule-related keyword arguments at all.
    let diagnostic = if let Some(keyword) = arguments.keywords.iter().find(|keyword| {
        let Keyword { arg, .. } = keyword;
        arg.as_ref()
            .is_some_and(|arg| matches!(arg.as_str(), "timetable" | "schedule_interval"))
    }) {
        // A deprecated argument is used.
        Diagnostic::new(
            AirflowDagNoScheduleArgument {
                deprecated_argument: keyword.arg.as_ref().map(ToString::to_string),
            },
            keyword.range(),
        )
    } else {
        // The implicit default is used.
        Diagnostic::new(
            AirflowDagNoScheduleArgument {
                deprecated_argument: None,
            },
            expr.range(),
        )
    };
    checker.diagnostics.push(diagnostic);
}
