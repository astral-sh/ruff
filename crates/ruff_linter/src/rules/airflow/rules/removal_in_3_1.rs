use crate::checkers::ast::Checker;
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
/// Checks for uses of deprecated Airflow functions and values.
///
/// ## Why is this bad?
/// Airflow 3.1 removed various deprecated functions, members, and other
/// values. Some have more modern replacements. Others are considered too niche
/// and not worth continued maintenance in Airflow.
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
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct Airflow31Removal {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow31Removal {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow31Removal {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::None
            | Replacement::AttrName(_)
            | Replacement::Message(_)
            | Replacement::Rename { module: _, name: _ }
            | Replacement::SourceModuleMoved { module: _, name: _ } => {
                format!("`{deprecated}` is removed in Airflow 3.1")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow31Removal { replacement, .. } = self;
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
        }
    }
}

/// AIR321
pub(crate) fn airflow_3_1_removal_expr(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Attribute(ExprAttribute { range, .. }) => {
            check_name(checker, expr, *range);
        }
        Expr::Name(ExprName { range, .. }) => {
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
        // airflow.utils
        [
            "airflow",
            "utils",
            "setup_teardown",
            "BaseSetupTeardownContext",
        ] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.setup_teardown",
            name: "BaseSetupTeardownContext",
        },
        ["airflow", "utils", "setup_teardown", "SetupTeardownContext"] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.setup_teardown",
            name: "SetupTeardownContext",
        },
        // airflow.utils.xcom
        ["airflow", "utils", "xcom", "XCOM_RETURN_KEY"] => Replacement::Rename {
            module: "airflow.models.xcom",
            name: "XCOM_RETURN_KEY",
        },
        // airflow.utils.task_group
        ["airflow", "utils", "task_group", "TaskGroup"] => Replacement::Rename {
            module: "airflow.sdk",
            name: "TaskGroup",
        },
        // airflow.utils.timezone
        ["airflow", "utils", "timezone", "coerce_datetime"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "coerce_datetime",
        },
        ["airflow", "utils", "timezone", "convert_to_utc"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "convert_to_utc",
        },
        ["airflow", "utils", "timezone", "datetime"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "datetime",
        },
        ["airflow", "utils", "timezone", "make_naive"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "make_naive",
        },
        ["airflow", "utils", "timezone", "parse"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "parse",
        },
        ["airflow", "utils", "timezone", "utc"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "utc",
        },
        ["airflow", "utils", "timezone", "utcnow"] => Replacement::Rename {
            module: "airflow.sdk.timezone",
            name: "utcnow",
        },
        // airflow.utils.decorators
        ["airflow", "utils", "decorators", "remove_task_decorator"] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.decorators",
            name: "remove_task_decorator",
        },
        [
            "airflow",
            "utils",
            "decorators",
            "fixup_decorator_warning_stack",
        ] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.decorators",
            name: "fixup_decorator_warning_stack",
        },
        // airflow.models.abstractoperator
        ["airflow", "models", "abstractoperator", "AbstractOperator"] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.abstractoperator",
            name: "AbstractOperator",
        },
        ["airflow", "models", "abstractoperator", "NotMapped"] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.abstractoperator",
            name: "NotMapped",
        },
        [
            "airflow",
            "models",
            "abstractoperator",
            "TaskStateChangeCallback",
        ] => Replacement::Rename {
            module: "airflow.sdk.definitions._internal.abstractoperator",
            name: "TaskStateChangeCallback",
        },
        // airflow.models.baseoperator
        ["airflow", "models", "baseoperator", "BaseOperator"] => Replacement::Rename {
            module: "airflow.sdk.bases.operator",
            name: "BaseOperator",
        },
        // airflow.macros
        ["airflow", "macros", "ds_add"] => Replacement::Rename {
            module: "airflow.sdk.execution_time.macros",
            name: "ds_add",
        },
        ["airflow", "macros", "ds_format"] => Replacement::Rename {
            module: "airflow.sdk.execution_time.macros",
            name: "ds_format",
        },
        ["airflow", "macros", "datetime_diff_for_humans"] => Replacement::Rename {
            module: "airflow.sdk.execution_time.macros",
            name: "datetime_diff_for_humans",
        },
        ["airflow", "macros", "ds_format_locale"] => Replacement::Rename {
            module: "airflow.sdk.execution_time.macros",
            name: "ds_format_locale",
        },
        // airflow.io
        ["airflow", "io", "get_fs"] => Replacement::Rename {
            module: "airflow.sdk.io",
            name: "get_fs",
        },
        ["airflow", "io", "has_fs"] => Replacement::Rename {
            module: "airflow.sdk.io",
            name: "has_fs",
        },
        ["airflow", "io", "Properties"] => Replacement::Rename {
            module: "airflow.sdk.io",
            name: "Properties",
        },
        _ => return,
    };

    let (module, name) = match &replacement {
        Replacement::Rename { module, name } => (module, *name),
        Replacement::SourceModuleMoved { module, name } => (module, name.as_str()),
        _ => {
            checker.report_diagnostic(
                Airflow31Removal {
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
        Airflow31Removal {
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
