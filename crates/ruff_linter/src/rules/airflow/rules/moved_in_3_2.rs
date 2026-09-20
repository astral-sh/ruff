use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::airflow::helpers::{
    Replacement, generate_import_edit, generate_remove_and_runtime_import_edit,
    is_guarded_by_try_except,
};
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprAttribute, ExprName};
use ruff_python_semantic::Modules;
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for uses of exceptions that were moved out of `airflow.exceptions`
/// in Airflow 3.2.
///
/// ## Why is this bad?
/// Airflow 3.2 moved a number of exceptions from `airflow.exceptions` to
/// `airflow.sdk.exceptions`. Importing them from `airflow.exceptions` still works,
/// but emits a `DeprecatedImportWarning`, and the compatibility shim will be
/// removed in a future version.
///
/// The replacement requires `apache-airflow-task-sdk>=1.2.0`.
///
/// ## Example
/// ```python
/// from airflow.exceptions import AirflowSkipException
///
/// raise AirflowSkipException("nothing to do")
/// ```
///
/// Use instead:
/// ```python
/// from airflow.sdk.exceptions import AirflowSkipException
///
/// raise AirflowSkipException("nothing to do")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Pedantic)]
pub(crate) struct Airflow32Moved {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow32Moved {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow32Moved { deprecated, .. } = self;
        format!("`{deprecated}` is moved in Airflow 3.2")
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow32Moved { replacement, .. } = self;
        match replacement {
            Replacement::SourceModuleMovedToSDK {
                module,
                name,
                version,
            } => Some(format!(
                "`{name}` has been moved to `{module}` since Airflow 3.2 (with apache-airflow-task-sdk>={version})."
            )),
            _ => None,
        }
    }
}

/// AIR322
pub(crate) fn airflow_3_2_moved_expr(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Attribute(ExprAttribute { range, .. }) | Expr::Name(ExprName { range, .. }) => {
            check_name(checker, expr, *range);
        }
        _ => {}
    }
}

/// Check whether an exception that was moved out of `airflow.exceptions` is used.
///
/// For example:
///
/// ```python
/// from airflow import exceptions
/// from airflow.exceptions import AirflowSkipException
///
/// # Accessing via attribute
/// raise exceptions.AirflowSkipException()
///
/// # Or, directly
/// raise AirflowSkipException()
/// ```
fn check_name(checker: &Checker, expr: &Expr, range: TextRange) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let [
        "airflow",
        "exceptions",
        name @ ("AirflowDagCycleException"
        | "AirflowFailException"
        | "AirflowInactiveAssetInInletOrOutletException"
        | "AirflowSensorTimeout"
        | "AirflowSkipException"
        | "AirflowTaskTerminated"
        | "AirflowTaskTimeout"
        | "DagRunTriggerException"
        | "DownstreamTasksSkipped"
        | "DuplicateTaskIdFound"
        | "FailFastDagInvalidTriggerRule"
        | "ParamValidationError"
        | "TaskAlreadyInTaskGroup"
        | "TaskDeferralError"
        | "TaskDeferralTimeout"
        | "TaskDeferred"
        | "XComNotFound"),
    ] = qualified_name.segments()
    else {
        return;
    };

    let module = "airflow.sdk.exceptions";
    let replacement = Replacement::SourceModuleMovedToSDK {
        module,
        name: (*name).to_string(),
        version: "1.2.0",
    };

    if is_guarded_by_try_except(expr, module, name, semantic) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        Airflow32Moved {
            deprecated: qualified_name.to_string(),
            replacement,
        },
        range,
    );

    if let Some(fix) = generate_import_edit(expr, checker, module, name, range)
        .or_else(|| generate_remove_and_runtime_import_edit(expr, checker, module, name))
    {
        diagnostic.set_fix(fix);
    }
}
