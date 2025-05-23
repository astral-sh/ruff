use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::airflow::helpers::{
    Replacement, is_airflow_builtin_or_provider, is_guarded_by_try_except,
};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{
    Arguments, Expr, ExprAttribute, ExprCall, ExprContext, ExprName, ExprStringLiteral,
    ExprSubscript, Stmt, StmtClassDef, StmtFunctionDef, name::QualifiedName,
};
use ruff_python_semantic::Modules;
use ruff_python_semantic::ScopeKind;
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for uses of deprecated Airflow functions and values.
///
/// ## Why is this bad?
/// Airflow 3.0 removed various deprecated functions, members, and other
/// values. Some have more modern replacements. Others are considered too niche
/// and not worth to be maintained in Airflow.
///
/// ## Example
/// ```python
/// from airflow.utils.dates import days_ago
///
///
/// yesterday = days_ago(today, 1)
/// ```
///
/// Use instead:
/// ```python
/// from datetime import timedelta
///
///
/// yesterday = today - timedelta(days=1)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct Airflow3Removal {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow3Removal {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3Removal {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::None
            | Replacement::AttrName(_)
            | Replacement::Message(_)
            | Replacement::AutoImport { module: _, name: _ }
            | Replacement::SourceModuleMoved { module: _, name: _ } => {
                format!("`{deprecated}` is removed in Airflow 3.0")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3Removal { replacement, .. } = self;
        match replacement {
            Replacement::None => None,
            Replacement::AttrName(name) => Some(format!("Use `{name}` instead")),
            Replacement::Message(message) => Some((*message).to_string()),
            Replacement::AutoImport { module, name } => {
                Some(format!("Use `{module}.{name}` instead"))
            }
            Replacement::SourceModuleMoved { module, name } => {
                Some(format!("Use `{module}.{name}` instead"))
            }
        }
    }
}

/// AIR301
pub(crate) fn airflow_3_removal_expr(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Call(
            call_expr @ ExprCall {
                func, arguments, ..
            },
        ) => {
            if let Some(qualified_name) = checker.semantic().resolve_qualified_name(func) {
                check_call_arguments(checker, &qualified_name, arguments);
            }
            check_method(checker, call_expr);
            check_context_key_usage_in_call(checker, call_expr);
        }
        Expr::Attribute(attribute_expr @ ExprAttribute { range, .. }) => {
            check_name(checker, expr, *range);
            check_class_attribute(checker, attribute_expr);
        }
        Expr::Name(ExprName { id, ctx, range }) => {
            check_name(checker, expr, *range);
            if matches!(ctx, ExprContext::Store) {
                if let ScopeKind::Class(class_def) = checker.semantic().current_scope().kind {
                    check_airflow_plugin_extension(checker, expr, id, class_def);
                }
            }
        }
        Expr::Subscript(subscript_expr) => {
            check_context_key_usage_in_subscript(checker, subscript_expr);
        }
        _ => {}
    }
}

/// AIR301
pub(crate) fn airflow_3_removal_function_def(checker: &Checker, function_def: &StmtFunctionDef) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    check_function_parameters(checker, function_def);
}

const REMOVED_CONTEXT_KEYS: [&str; 12] = [
    "conf",
    "execution_date",
    "next_ds",
    "next_ds_nodash",
    "next_execution_date",
    "prev_ds",
    "prev_ds_nodash",
    "prev_execution_date",
    "prev_execution_date_success",
    "tomorrow_ds",
    "yesterday_ds",
    "yesterday_ds_nodash",
];

/// Check the function parameters for removed context keys.
///
/// For example:
///
/// ```python
/// from airflow.decorators import task
///
/// @task
/// def another_task(execution_date, **kwargs):
///     #            ^^^^^^^^^^^^^^
///     #            'execution_date' is removed in Airflow 3.0
///     pass
/// ```
fn check_function_parameters(checker: &Checker, function_def: &StmtFunctionDef) {
    if !is_airflow_task(function_def, checker.semantic())
        && !is_execute_method_inherits_from_airflow_operator(function_def, checker.semantic())
    {
        return;
    }

    for param in function_def.parameters.iter_non_variadic_params() {
        let param_name = param.name();
        if REMOVED_CONTEXT_KEYS.contains(&param_name.as_str()) {
            checker.report_diagnostic(Diagnostic::new(
                Airflow3Removal {
                    deprecated: param_name.to_string(),
                    replacement: Replacement::None,
                },
                param_name.range(),
            ));
        }
    }
}

/// Check whether a removed Airflow argument is passed.
///
/// For example:
///
/// ```python
/// from airflow import DAG
///
/// DAG(schedule_interval="@daily")
/// ```
fn check_call_arguments(checker: &Checker, qualified_name: &QualifiedName, arguments: &Arguments) {
    match qualified_name.segments() {
        ["airflow", .., "DAG" | "dag"] => {
            // with replacement
            diagnostic_for_argument(checker, arguments, "fail_stop", Some("fail_fast"));
            diagnostic_for_argument(checker, arguments, "schedule_interval", Some("schedule"));
            diagnostic_for_argument(checker, arguments, "timetable", Some("schedule"));
            // without replacement
            diagnostic_for_argument(checker, arguments, "default_view", None);
            diagnostic_for_argument(checker, arguments, "orientation", None);
        }
        segments => {
            if is_airflow_auth_manager(segments) {
                if !arguments.is_empty() {
                    checker.report_diagnostic(Diagnostic::new(
                        Airflow3Removal {
                            deprecated: String::from("appbuilder"),
                            replacement: Replacement::Message(
                                "The constructor takes no parameter now",
                            ),
                        },
                        arguments.range(),
                    ));
                }
            } else if is_airflow_task_handler(segments) {
                diagnostic_for_argument(checker, arguments, "filename_template", None);
            } else if is_airflow_builtin_or_provider(segments, "operators", "Operator") {
                diagnostic_for_argument(
                    checker,
                    arguments,
                    "task_concurrency",
                    Some("max_active_tis_per_dag"),
                );
                match segments {
                    [
                        "airflow",
                        ..,
                        "operators",
                        "trigger_dagrun",
                        "TriggerDagRunOperator",
                    ] => {
                        diagnostic_for_argument(
                            checker,
                            arguments,
                            "execution_date",
                            Some("logical_date"),
                        );
                    }
                    [
                        "airflow",
                        ..,
                        "operators",
                        "datetime",
                        "BranchDateTimeOperator",
                    ]
                    | [
                        "airflow",
                        ..,
                        "operators",
                        "weekday",
                        "BranchDayOfWeekOperator",
                    ]
                    | ["airflow", .., "sensors", "weekday", "DayOfWeekSensor"] => {
                        diagnostic_for_argument(
                            checker,
                            arguments,
                            "use_task_execution_day",
                            Some("use_task_logical_date"),
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Check whether a removed Airflow class attribute (include property) is called.
///
/// For example:
///
/// ```python
/// from airflow.linesage.hook import DatasetLineageInfo
///
/// info = DatasetLineageInfo()
/// info.dataset
/// ```
fn check_class_attribute(checker: &Checker, attribute_expr: &ExprAttribute) {
    let ExprAttribute { value, attr, .. } = attribute_expr;

    let Some(qualname) = typing::resolve_assignment(value, checker.semantic()) else {
        return;
    };

    let replacement = match *qualname.segments() {
        ["airflow", "providers_manager", "ProvidersManager"] => match attr.as_str() {
            "dataset_factories" => Replacement::AttrName("asset_factories"),
            "dataset_uri_handlers" => Replacement::AttrName("asset_uri_handlers"),
            "dataset_to_openlineage_converters" => {
                Replacement::AttrName("asset_to_openlineage_converters")
            }
            _ => return,
        },
        ["airflow", "lineage", "hook", "DatasetLineageInfo"] => match attr.as_str() {
            "dataset" => Replacement::AttrName("asset"),
            _ => return,
        },
        _ => return,
    };

    // Create the `Fix` first to avoid cloning `Replacement`.
    let fix = if let Replacement::AttrName(name) = replacement {
        Some(Fix::safe_edit(Edit::range_replacement(
            name.to_string(),
            attr.range(),
        )))
    } else {
        None
    };
    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: attr.to_string(),
            replacement,
        },
        attr.range(),
    );
    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
    checker.report_diagnostic(diagnostic);
}

/// Checks whether an Airflow 3.0–removed context key is used in a function decorated with `@task`.
///
/// Specifically, it flags the following two scenarios:
///
/// 1. A removed context key accessed via `context.get("...")` where context is coming from
///    `get_current_context` function.
///
/// ```python
/// from airflow.decorators import task
/// from airflow.utils.context import get_current_context
///
///
/// @task
/// def my_task():
///     context = get_current_context()
///     context.get("conf")  # 'conf' is removed in Airflow 3.0
/// ```
///
/// 2. A removed context key accessed via `context.get("...")` where context is a kwarg parameter.
///
/// ```python
/// from airflow.decorators import task
///
///
/// @task
/// def my_task(**context):
///     context.get("conf")  # 'conf' is removed in Airflow 3.0
/// ```
fn check_context_key_usage_in_call(checker: &Checker, call_expr: &ExprCall) {
    if !in_airflow_task_function(checker.semantic()) {
        return;
    }

    let Expr::Attribute(ExprAttribute { value, attr, .. }) = &*call_expr.func else {
        return;
    };

    if attr.as_str() != "get" {
        return;
    }

    let is_kwarg_parameter = value
        .as_name_expr()
        .is_some_and(|name| is_kwarg_parameter(checker.semantic(), name));

    let is_assigned_from_get_current_context =
        typing::resolve_assignment(value, checker.semantic()).is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["airflow", "utils", "context", "get_current_context"]
            )
        });

    if !(is_kwarg_parameter || is_assigned_from_get_current_context) {
        return;
    }

    for removed_key in REMOVED_CONTEXT_KEYS {
        let Some(Expr::StringLiteral(ExprStringLiteral { value, range })) =
            call_expr.arguments.find_positional(0)
        else {
            continue;
        };
        if value == removed_key {
            checker.report_diagnostic(Diagnostic::new(
                Airflow3Removal {
                    deprecated: removed_key.to_string(),
                    replacement: Replacement::None,
                },
                *range,
            ));
        }
    }
}

/// Check if a subscript expression accesses a removed Airflow context variable.
/// If a removed key is found, push a corresponding diagnostic.
fn check_context_key_usage_in_subscript(checker: &Checker, subscript: &ExprSubscript) {
    if !in_airflow_task_function(checker.semantic()) {
        return;
    }

    let ExprSubscript { value, slice, .. } = subscript;

    let Some(ExprStringLiteral { value: key, .. }) = slice.as_string_literal_expr() else {
        return;
    };

    let is_kwarg_parameter = value
        .as_name_expr()
        .is_some_and(|name| is_kwarg_parameter(checker.semantic(), name));

    let is_assigned_from_get_current_context =
        typing::resolve_assignment(value, checker.semantic()).is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["airflow", "utils", "context", "get_current_context"]
            )
        });

    if !(is_kwarg_parameter || is_assigned_from_get_current_context) {
        return;
    }

    if REMOVED_CONTEXT_KEYS.contains(&key.to_str()) {
        checker.report_diagnostic(Diagnostic::new(
            Airflow3Removal {
                deprecated: key.to_string(),
                replacement: Replacement::None,
            },
            slice.range(),
        ));
    }
}

/// Finds the parameter definition for a given name expression in a function.
fn is_kwarg_parameter(semantic: &SemanticModel, name: &ExprName) -> bool {
    let Some(binding_id) = semantic.only_binding(name) else {
        return false;
    };
    let binding = semantic.binding(binding_id);
    let Some(Stmt::FunctionDef(StmtFunctionDef { parameters, .. })) = binding.statement(semantic)
    else {
        return false;
    };
    parameters
        .kwarg
        .as_deref()
        .is_some_and(|kwarg| kwarg.name.as_str() == name.id.as_str())
}

/// Check whether a removed Airflow class method is called.
///
/// For example:
///
/// ```python
/// from airflow.datasets.manager import DatasetManager
///
/// manager = DatasetManager()
/// manager.register_datsaet_change()
/// ```
fn check_method(checker: &Checker, call_expr: &ExprCall) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = &*call_expr.func else {
        return;
    };

    let Some(qualname) = typing::resolve_assignment(value, checker.semantic()) else {
        return;
    };

    let replacement = match qualname.segments() {
        ["airflow", "datasets", "manager", "DatasetManager"] => match attr.as_str() {
            "register_dataset_change" => Replacement::AttrName("register_asset_change"),
            "create_datasets" => Replacement::AttrName("create_assets"),
            "notify_dataset_created" => Replacement::AttrName("notify_asset_created"),
            "notify_dataset_changed" => Replacement::AttrName("notify_asset_changed"),
            "notify_dataset_alias_created" => Replacement::AttrName("notify_asset_alias_created"),
            _ => return,
        },
        ["airflow", "lineage", "hook", "HookLineageCollector"] => match attr.as_str() {
            "create_dataset" => Replacement::AttrName("create_asset"),
            "add_input_dataset" => Replacement::AttrName("add_input_asset"),
            "add_output_dataset" => Replacement::AttrName("add_output_asset"),
            "collected_datasets" => Replacement::AttrName("collected_assets"),
            _ => return,
        },
        ["airflow", "providers_manager", "ProvidersManager"] => match attr.as_str() {
            "initialize_providers_dataset_uri_resources" => {
                Replacement::AttrName("initialize_providers_asset_uri_resources")
            }
            _ => return,
        },
        [
            "airflow",
            "secrets",
            "local_filesystem",
            "LocalFilesystemBackend",
        ] => match attr.as_str() {
            "get_connections" => Replacement::AttrName("get_connection"),
            _ => return,
        },
        ["airflow", "datasets", ..] | ["airflow", "Dataset"] => match attr.as_str() {
            "iter_datasets" => Replacement::AttrName("iter_assets"),
            "iter_dataset_aliases" => Replacement::AttrName("iter_asset_aliases"),
            _ => return,
        },
        segments => {
            if is_airflow_secret_backend(segments) {
                match attr.as_str() {
                    "get_conn_uri" => Replacement::AttrName("get_conn_value"),
                    "get_connections" => Replacement::AttrName("get_connection"),
                    _ => return,
                }
            } else if is_airflow_hook(segments) {
                match attr.as_str() {
                    "get_connections" => Replacement::AttrName("get_connection"),
                    _ => return,
                }
            } else if is_airflow_auth_manager(segments) {
                if attr.as_str() == "is_authorized_dataset" {
                    Replacement::AttrName("is_authorized_asset")
                } else {
                    return;
                }
            } else {
                return;
            }
        }
    };
    // Create the `Fix` first to avoid cloning `Replacement`.
    let fix = if let Replacement::AttrName(name) = replacement {
        Some(Fix::safe_edit(Edit::range_replacement(
            name.to_string(),
            attr.range(),
        )))
    } else {
        None
    };

    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: attr.to_string(),
            replacement,
        },
        attr.range(),
    );
    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
    checker.report_diagnostic(diagnostic);
}

/// Check whether a removed Airflow name is used.
///
/// For example:
///
/// ```python
/// from airflow.operators import subdag
/// from airflow.operators.subdag import SubDagOperator
///
/// # Accessing via attribute
/// subdag.SubDagOperator()
///
/// # Or, directly
/// SubDagOperator()
/// ```
fn check_name(checker: &Checker, expr: &Expr, range: TextRange) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        // airflow.PY\d{1,2}
        [
            "airflow",
            "PY36" | "PY37" | "PY38" | "PY39" | "PY310" | "PY311" | "PY312",
        ] => Replacement::Message("Use `sys.version_info` instead"),

        // airflow.api_connexion.security
        ["airflow", "api_connexion", "security", "requires_access"] => Replacement::Message(
            "Use `airflow.api_fastapi.core_api.security.requires_access_*` instead",
        ),
        [
            "airflow",
            "api_connexion",
            "security",
            "requires_access_dataset",
        ] => Replacement::AutoImport {
            module: "airflow.api_fastapi.core_api.security",
            name: "requires_access_asset",
        },

        // airflow.auth.managers
        [
            "airflow",
            "auth",
            "managers",
            "base_auth_manager",
            "BaseAuthManager",
        ] => Replacement::AutoImport {
            module: "airflow.api_fastapi.auth.managers.base_auth_manager",
            name: "BaseAuthManager",
        },
        [
            "airflow",
            "auth",
            "managers",
            "models",
            "resource_details",
            "DatasetDetails",
        ] => Replacement::AutoImport {
            module: "airflow.api_fastapi.auth.managers.models.resource_details",
            name: "AssetDetails",
        },

        // airflow.configuration
        // TODO: check whether we could improve it
        [
            "airflow",
            "configuration",
            rest @ ("as_dict" | "get" | "getboolean" | "getfloat" | "getint" | "has_option"
            | "remove_option" | "set"),
        ] => Replacement::SourceModuleMoved {
            module: "airflow.configuration",
            name: format!("conf.{rest}"),
        },

        // airflow.contrib.*
        ["airflow", "contrib", ..] => {
            Replacement::Message("The whole `airflow.contrib` module has been removed.")
        }

        // airflow.datasets.manager
        ["airflow", "datasets", "manager", rest] => match *rest {
            "DatasetManager" => Replacement::AutoImport {
                module: "airflow.assets.manager",
                name: "AssetManager",
            },
            "dataset_manager" => Replacement::AutoImport {
                module: "airflow.assets.manager",
                name: "asset_manager",
            },
            "resolve_dataset_manager" => Replacement::AutoImport {
                module: "airflow.assets.manager",
                name: "resolve_asset_manager",
            },
            _ => return,
        },
        // airflow.datasets
        ["airflow", "datasets", "DatasetAliasEvent"] => Replacement::None,

        // airflow.hooks
        ["airflow", "hooks", "base_hook", "BaseHook"] => Replacement::AutoImport {
            module: "airflow.hooks.base",
            name: "BaseHook",
        },

        // airflow.lineage.hook
        ["airflow", "lineage", "hook", "DatasetLineageInfo"] => Replacement::AutoImport {
            module: "airflow.lineage.hook",
            name: "AssetLineageInfo",
        },

        // airflow.listeners.spec
        ["airflow", "listeners", "spec", "dataset", rest] => match *rest {
            "on_dataset_created" => Replacement::AutoImport {
                module: "airflow.listeners.spec.asset",
                name: "on_asset_created",
            },
            "on_dataset_changed" => Replacement::AutoImport {
                module: "airflow.listeners.spec.asset",
                name: "on_asset_changed",
            },
            _ => return,
        },

        // airflow.metrics.validators
        ["airflow", "metrics", "validators", rest] => match *rest {
            "AllowListValidator" => Replacement::AutoImport {
                module: "airflow.metrics.validators",
                name: "PatternAllowListValidator",
            },
            "BlockListValidator" => Replacement::AutoImport {
                module: "airflow.metrics.validators",
                name: "PatternBlockListValidator",
            },
            _ => return,
        },

        // airflow.notifications
        ["airflow", "notifications", "basenotifier", "BaseNotifier"] => Replacement::AutoImport {
            module: "airflow.sdk.bases.notifier",
            name: "BaseNotifier",
        },

        // airflow.operators
        ["airflow", "operators", "subdag", ..] => {
            Replacement::Message("The whole `airflow.subdag` module has been removed.")
        }
        ["airflow", "operators", "python", "get_current_context"] => Replacement::AutoImport {
            module: "airflow.sdk",
            name: "get_current_context",
        },

        // airflow.secrets
        ["airflow", "secrets", "local_filesystem", "load_connections"] => Replacement::AutoImport {
            module: "airflow.secrets.local_filesystem",
            name: "load_connections_dict",
        },

        // airflow.security
        ["airflow", "security", "permissions", "RESOURCE_DATASET"] => Replacement::AutoImport {
            module: "airflow.security.permissions",
            name: "RESOURCE_ASSET",
        },

        // airflow.sensors
        [
            "airflow",
            "sensors",
            "base_sensor_operator",
            "BaseSensorOperator",
        ] => Replacement::AutoImport {
            module: "airflow.sdk.bases.sensor",
            name: "BaseSensorOperator",
        },

        // airflow.timetables
        [
            "airflow",
            "timetables",
            "simple",
            "DatasetTriggeredTimetable",
        ] => Replacement::AutoImport {
            module: "airflow.timetables.simple",
            name: "AssetTriggeredTimetable",
        },

        // airflow.triggers
        ["airflow", "triggers", "external_task", "TaskStateTrigger"] => Replacement::None,

        // airflow.utils
        ["airflow", "utils", rest @ ..] => match &rest {
            // airflow.utils.dag_cycle_tester
            ["dag_cycle_tester", "test_cycle"] => Replacement::None,

            // airflow.utils.db
            ["db", "create_session"] => Replacement::None,

            // airflow.utils.decorators
            ["decorators", "apply_defaults"] => Replacement::Message(
                "`apply_defaults` is now unconditionally done and can be safely removed.",
            ),

            // airflow.utils.dates
            ["dates", "date_range"] => Replacement::None,
            ["dates", "days_ago"] => {
                Replacement::Message("Use `pendulum.today('UTC').add(days=-N, ...)` instead")
            }
            [
                "dates",
                "parse_execution_date" | "round_time" | "scale_time_units" | "infer_time_unit",
            ] => Replacement::None,

            // airflow.utils.file
            ["file", "TemporaryDirectory"] => Replacement::AutoImport {
                module: "tempfile",
                name: "TemporaryDirectory",
            },
            ["file", "mkdirs"] => Replacement::Message("Use `pathlib.Path({path}).mkdir` instead"),

            // airflow.utils.helpers
            ["helpers", "chain"] => Replacement::AutoImport {
                module: "airflow.sdk",
                name: "chain",
            },
            ["helpers", "cross_downstream"] => Replacement::AutoImport {
                module: "airflow.sdk",
                name: "cross_downstream",
            },

            // TODO: update it as SourceModuleMoved
            // airflow.utils.log.secrets_masker
            ["log", "secrets_masker"] => Replacement::AutoImport {
                module: "airflow.sdk.execution_time",
                name: "secrets_masker",
            },

            // airflow.utils.state
            ["state", "SHUTDOWN" | "terminating_states"] => Replacement::None,

            // airflow.utils.trigger_rule
            [
                "trigger_rule",
                "TriggerRule",
                "DUMMY" | "NONE_FAILED_OR_SKIPPED",
            ] => Replacement::None,
            _ => return,
        },

        // airflow.www
        [
            "airflow",
            "www",
            "auth",
            "has_access" | "has_access_dataset",
        ] => Replacement::None,
        [
            "airflow",
            "www",
            "utils",
            "get_sensitive_variables_fields" | "should_hide_value_for_key",
        ] => Replacement::None,

        // airflow.providers.amazon
        [
            "airflow",
            "providers",
            "amazon",
            "aws",
            "datasets",
            "s3",
            rest,
        ] => match *rest {
            "create_dataset" => Replacement::AutoImport {
                module: "airflow.providers.amazon.aws.assets.s3",
                name: "create_asset",
            },
            "convert_dataset_to_openlineage" => Replacement::AutoImport {
                module: "airflow.providers.amazon.aws.assets.s3",
                name: "convert_asset_to_openlineage",
            },
            "sanitize_uri" => Replacement::AutoImport {
                module: "airflow.providers.amazon.aws.assets.s3",
                name: "sanitize_uri",
            },
            _ => return,
        },
        [
            "airflow",
            "providers",
            "amazon",
            "aws",
            "auth_manager",
            "avp",
            "entities",
            "AvpEntities",
            "DATASET",
        ] => Replacement::AutoImport {
            module: "airflow.providers.amazon.aws.auth_manager.avp.entities",
            name: "AvpEntities.ASSET",
        },

        // airflow.providers.common.io
        // airflow.providers.common.io.datasets.file
        [
            "airflow",
            "providers",
            "common",
            "io",
            "datasets",
            "file",
            rest,
        ] => match *rest {
            "create_dataset" => Replacement::AutoImport {
                module: "airflow.providers.common.io.assets.file",
                name: "create_asset",
            },
            "convert_dataset_to_openlineage" => Replacement::AutoImport {
                module: "airflow.providers.common.io.assets.file",
                name: "convert_asset_to_openlineage",
            },
            "sanitize_uri" => Replacement::AutoImport {
                module: "airflow.providers.common.io.assets.file",
                name: "sanitize_uri",
            },
            _ => return,
        },

        // airflow.providers.google
        // airflow.providers.google.datasets
        ["airflow", "providers", "google", "datasets", rest @ ..] => match &rest {
            ["bigquery", "create_dataset"] => Replacement::AutoImport {
                module: "airflow.providers.google.assets.bigquery",
                name: "create_asset",
            },
            ["gcs", "create_dataset"] => Replacement::AutoImport {
                module: "airflow.providers.google.assets.gcs",
                name: "create_asset",
            },
            ["gcs", "convert_dataset_to_openlineage"] => Replacement::AutoImport {
                module: "airflow.providers.google.assets.gcs",
                name: "convert_asset_to_openlineage",
            },
            ["gcs", "sanitize_uri"] => Replacement::AutoImport {
                module: "airflow.providers.google.assets.gcs",
                name: "sanitize_uri",
            },

            _ => return,
        },

        // airflow.providers.mysql
        [
            "airflow",
            "providers",
            "mysql",
            "datasets",
            "mysql",
            "sanitize_uri",
        ] => Replacement::AutoImport {
            module: "airflow.providers.mysql.assets.mysql",
            name: "sanitize_uri",
        },

        // airflow.providers.postgres
        [
            "airflow",
            "providers",
            "postgres",
            "datasets",
            "postgres",
            "sanitize_uri",
        ] => Replacement::AutoImport {
            module: "airflow.providers.postgres.assets.postgres",
            name: "sanitize_uri",
        },

        // airflow.providers.openlineage
        // airflow.providers.openlineage.utils.utils
        [
            "airflow",
            "providers",
            "openlineage",
            "utils",
            "utils",
            rest,
        ] => match *rest {
            "DatasetInfo" => Replacement::AutoImport {
                module: "airflow.providers.openlineage.utils.utils",
                name: "AssetInfo",
            },

            "translate_airflow_dataset" => Replacement::AutoImport {
                module: "airflow.providers.openlineage.utils.utils",
                name: "translate_airflow_asset",
            },
            _ => return,
        },

        // airflow.providers.trino
        [
            "airflow",
            "providers",
            "trino",
            "datasets",
            "trino",
            "sanitize_uri",
        ] => Replacement::AutoImport {
            module: "airflow.providers.trino.assets.trino",
            name: "sanitize_uri",
        },

        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: qualified_name.to_string(),
            replacement: replacement.clone(),
        },
        range,
    );
    let semantic = checker.semantic();
    if let Some((module, name)) = match &replacement {
        Replacement::AutoImport { module, name } => Some((module, *name)),
        Replacement::SourceModuleMoved { module, name } => Some((module, name.as_str())),
        _ => None,
    } {
        if is_guarded_by_try_except(expr, module, name, semantic) {
            return;
        }

        let import_target = name.split('.').next().unwrap_or(name);

        diagnostic.try_set_fix(|| {
            let (import_edit, _) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from(module, import_target),
                expr.start(),
                checker.semantic(),
            )?;
            let replacement_edit = Edit::range_replacement(name.to_string(), range);
            Ok(Fix::safe_edits(import_edit, [replacement_edit]))
        });
    }

    checker.report_diagnostic(diagnostic);
}

/// Check whether a customized Airflow plugin contains removed extensions.
///
/// For example:
///
/// ```python
/// from airflow.plugins_manager import AirflowPlugin
///
///
/// class CustomizePlugin(AirflowPlugin)
///     executors = "some.third.party.executor"
/// ```
fn check_airflow_plugin_extension(
    checker: &Checker,
    expr: &Expr,
    name: &str,
    class_def: &StmtClassDef,
) {
    if matches!(name, "executors" | "operators" | "sensors" | "hooks") {
        if class_def.bases().iter().any(|class_base| {
            checker
                .semantic()
                .resolve_qualified_name(class_base)
                .is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["airflow", "plugins_manager", "AirflowPlugin"]
                    )
                })
        }) {
            checker.report_diagnostic(Diagnostic::new(
                Airflow3Removal {
                    deprecated: name.to_string(),
                    replacement: Replacement::Message(
                        "This extension should just be imported as a regular python module.",
                    ),
                },
                expr.range(),
            ));
        }
    }
}

/// Check if the `deprecated` keyword argument is being used and create a diagnostic if so along
/// with a possible `replacement`.
fn diagnostic_for_argument(
    checker: &Checker,
    arguments: &Arguments,
    deprecated: &str,
    replacement: Option<&'static str>,
) {
    let Some(keyword) = arguments.find_keyword(deprecated) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: deprecated.to_string(),
            replacement: match replacement {
                Some(name) => Replacement::AttrName(name),
                None => Replacement::None,
            },
        },
        keyword
            .arg
            .as_ref()
            .map_or_else(|| keyword.range(), Ranged::range),
    );

    if let Some(replacement) = replacement {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            replacement.to_string(),
            diagnostic.range,
        )));
    }

    checker.report_diagnostic(diagnostic);
}

/// Check whether the symbol is coming from the `secrets` builtin or provider module which ends
/// with `Backend`.
fn is_airflow_secret_backend(segments: &[&str]) -> bool {
    is_airflow_builtin_or_provider(segments, "secrets", "Backend")
}

/// Check whether the symbol is coming from the `hooks` builtin or provider module which ends
/// with `Hook`.
fn is_airflow_hook(segments: &[&str]) -> bool {
    is_airflow_builtin_or_provider(segments, "hooks", "Hook")
}

/// Check whether the symbol is coming from the `log` builtin or provider module which ends
/// with `TaskHandler`.
fn is_airflow_task_handler(segments: &[&str]) -> bool {
    is_airflow_builtin_or_provider(segments, "log", "TaskHandler")
}

/// Check whether the symbol is coming from the `auth.manager` builtin or provider `auth_manager` module which ends
/// with `AuthManager`.
fn is_airflow_auth_manager(segments: &[&str]) -> bool {
    match segments {
        ["airflow", "auth", "manager", rest @ ..] => {
            if let Some(last_element) = rest.last() {
                last_element.ends_with("AuthManager")
            } else {
                false
            }
        }

        ["airflow", "providers", rest @ ..] => {
            if let (Some(pos), Some(last_element)) =
                (rest.iter().position(|&s| s == "auth_manager"), rest.last())
            {
                pos + 1 < rest.len() && last_element.ends_with("AuthManager")
            } else {
                false
            }
        }

        _ => false,
    }
}

/// Returns `true` if the current statement hierarchy has a function that's decorated with
/// `@airflow.decorators.task`.
fn in_airflow_task_function(semantic: &SemanticModel) -> bool {
    semantic
        .current_statements()
        .find_map(|stmt| stmt.as_function_def_stmt())
        .is_some_and(|function_def| is_airflow_task(function_def, semantic))
}

/// Returns `true` if the given function is decorated with `@airflow.decorators.task`.
fn is_airflow_task(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["airflow", "decorators", "task"])
            })
    })
}

/// Check it's "execute" method inherits from Airflow base operator
///
/// For example:
///
/// ```python
/// from airflow.models.baseoperator import BaseOperator
///
/// class CustomOperator(BaseOperator):
///     def execute(self):
///         pass
/// ```
fn is_execute_method_inherits_from_airflow_operator(
    function_def: &StmtFunctionDef,
    semantic: &SemanticModel,
) -> bool {
    if function_def.name.as_str() != "execute" {
        return false;
    }

    let ScopeKind::Class(class_def) = semantic.current_scope().kind else {
        return false;
    };

    class_def.bases().iter().any(|class_base| {
        semantic
            .resolve_qualified_name(class_base)
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["airflow", .., "BaseOperator"])
            })
    })
}
