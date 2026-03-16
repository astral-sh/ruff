use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::{
    INTERNAL_MODULE_WARNING, Replacement, generate_import_edit,
    generate_remove_and_runtime_import_edit, is_guarded_by_try_except,
};
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprAttribute, ExprName};
use ruff_python_semantic::Modules;
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for uses of deprecated or moved Airflow functions and values in Airflow 3.1.
///
/// ## Why is this bad?
/// Airflow 3.1 deprecated or moved various functions, members, and other values.
///
/// ## Example
/// ```python
/// from airflow.utils.timezone import convert_to_utc
/// from datetime import datetime
///
/// convert_to_utc(datetime.now())
/// ```
///
/// Use instead:
/// ```python
/// from airflow.sdk.timezone import convert_to_utc
/// from datetime import datetime
///
/// convert_to_utc(datetime.now())
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.1")]
pub(crate) struct Airflow31Moved {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow31Moved {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow31Moved { deprecated, .. } = self;
        format!("`{deprecated}` is moved in Airflow 3.1")
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow31Moved { replacement, .. } = self;
        match replacement {
            Replacement::None => None,
            Replacement::AttrName(name) => Some(format!("Use `{name}` instead")),
            Replacement::Message(message) => Some((*message).to_string()),
            Replacement::Rename { module, name } => {
                Some(format!("Use `{name}` from `{module}` instead."))
            }
            Replacement::SourceModuleMoved { module, name } => {
                Some(format!("Use `{name}` from `{module}` instead."))
            }
            Replacement::SourceModuleMovedToSDK {
                module,
                name,
                version,
            } => Some(format!(
                "`{name}` has been moved to `{module}` since Airflow 3.1 (with apache-airflow-task-sdk>={version})."
            )),
            Replacement::SourceModuleMovedWithMessage {
                module,
                name,
                message,
                ..
            } => Some(format!(
                "`{name}` has been moved to `{module}` since Airflow 3.1. {message}"
            )),
        }
    }
}

/// AIR321
pub(crate) fn airflow_3_1_moved_expr(checker: &Checker, expr: &Expr) {
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

/// Check whether a removed Airflow name is used.
///
/// For example:
///
/// ```python
/// from airflow.operators import timezone
/// from airflow.utils.timezone import convert_to_utc
/// from datetime import datetime
///
/// # Accessing via attribute
/// timezone.convert_to_utc(datetime.now())
///
/// # Or, directly
/// convert_to_utc(datetime.now())
/// ```
fn check_name(checker: &Checker, expr: &Expr, range: TextRange) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        // airflow.utils.setup_teardown
        [
            "airflow",
            "utils",
            "setup_teardown",
            rest @ ("BaseSetupTeardownContext" | "SetupTeardownContext"),
        ] => Replacement::SourceModuleMovedWithMessage {
            module: "airflow.sdk.definitions._internal.setup_teardown",
            name: rest.to_string(),
            message: INTERNAL_MODULE_WARNING,
            suggest_fix: false,
        },

        // airflow.secrets
        ["airflow", "secrets", "cache", "SecretCache"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: "SecretCache".to_string(),
            version: "1.1.0",
        },

        // airflow.utils.xcom
        ["airflow", "utils", "xcom", "XCOM_RETURN_KEY"] => {
            Replacement::SourceModuleMovedWithMessage {
                module: "airflow.models.xcom",
                name: "XCOM_RETURN_KEY".to_string(),
                message: INTERNAL_MODULE_WARNING,
                suggest_fix: false,
            }
        }

        // airflow.utils.task_group
        ["airflow", "utils", "task_group", "TaskGroup"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: "TaskGroup".to_string(),
            version: "1.1.0",
        },

        // airflow.utils.timezone
        [
            "airflow",
            "utils",
            "timezone",
            rest @ ("coerce_datetime" | "convert_to_utc" | "datetime" | "make_naive" | "parse"
            | "utc" | "utcnow"),
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk.timezone",
            name: rest.to_string(),
            version: "1.1.0",
        },

        // airflow.utils.decorators
        [
            "airflow",
            "utils",
            "decorators",
            rest @ ("remove_task_decorator" | "fixup_decorator_warning_stack"),
        ] => Replacement::SourceModuleMovedWithMessage {
            module: "airflow.sdk.definitions._internal.decorators",
            name: rest.to_string(),
            message: INTERNAL_MODULE_WARNING,
            suggest_fix: false,
        },

        // airflow.models.abstractoperator
        [
            "airflow",
            "models",
            "abstractoperator",
            rest @ ("AbstractOperator" | "NotMapped" | "TaskStateChangeCallback"),
        ] => Replacement::SourceModuleMovedWithMessage {
            module: "airflow.sdk.definitions._internal.abstractoperator",
            name: rest.to_string(),
            message: INTERNAL_MODULE_WARNING,
            suggest_fix: false,
        },

        // airflow.models.baseoperator
        ["airflow", "models", "baseoperator", "BaseOperator"] => {
            Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "BaseOperator".to_string(),
                version: "1.1.0",
            }
        }

        // airflow.macros
        [
            "airflow",
            "macros",
            rest @ ("ds_add" | "ds_format" | "datetime_diff_for_humans" | "ds_format_locale"),
        ] => Replacement::SourceModuleMovedWithMessage {
            module: "airflow.sdk.execution_time.macros",
            name: rest.to_string(),
            message: "Requires `apache-airflow-task-sdk>=1.1.0,<=1.1.6`. For `apache-airflow-task-sdk>=1.1.7`, import from `airflow.sdk` instead.",
            suggest_fix: true,
        },

        // airflow.io
        ["airflow", "io", rest @ ("get_fs" | "has_fs" | "Properties")] => {
            Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk.io",
                name: rest.to_string(),
                version: "1.1.0",
            }
        }

        // airflow.hooks
        ["airflow", "hooks", "base", "BaseHook"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: "BaseHook".to_string(),
            version: "1.1.0",
        },
        _ => return,
    };

    let (module, name) = match &replacement {
        Replacement::Rename { module, name } => (module, *name),
        Replacement::SourceModuleMoved { module, name } => (module, name.as_str()),
        Replacement::SourceModuleMovedToSDK { module, name, .. } => (module, name.as_str()),
        Replacement::SourceModuleMovedWithMessage {
            module,
            name,
            suggest_fix,
            ..
        } if *suggest_fix => (module, name.as_str()),
        _ => {
            checker.report_diagnostic(
                Airflow31Moved {
                    deprecated: qualified_name.to_string(),
                    replacement: replacement.clone(),
                },
                range,
            );
            return;
        }
    };

    if is_guarded_by_try_except(expr, module, name, checker.semantic()) {
        return;
    }

    let import_target = name.split('.').next().unwrap_or(name);
    let mut diagnostic = checker.report_diagnostic(
        Airflow31Moved {
            deprecated: qualified_name.to_string(),
            replacement: replacement.clone(),
        },
        range,
    );

    if let Some(fix) = generate_import_edit(expr, checker, module, import_target, range)
        .or_else(|| generate_remove_and_runtime_import_edit(expr, checker, module, name))
    {
        diagnostic.set_fix(fix);
    }
}
