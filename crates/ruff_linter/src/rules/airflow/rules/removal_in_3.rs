use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::airflow::helpers::{is_guarded_by_try_except, Replacement};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{
    name::QualifiedName, Arguments, Expr, ExprAttribute, ExprCall, ExprContext, ExprName,
    ExprStringLiteral, ExprSubscript, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::Modules;
use ruff_python_semantic::ScopeKind;
use ruff_python_semantic::SemanticModel;
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
            Replacement::None => format!("`{deprecated}` is removed in Airflow 3.0"),
            Replacement::Name(_) => {
                format!("`{deprecated}` is removed in Airflow 3.0")
            }
            Replacement::Message(message) => {
                format!("`{deprecated}` is removed in Airflow 3.0; {message}")
            }
            Replacement::AutoImport { path: _, name: _ } => {
                format!("`{deprecated}` is removed in Airflow 3.0")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3Removal { replacement, .. } = self;
        match replacement {
            Replacement::Name(name) => Some(format!("Use `{name}` instead")),
            Replacement::AutoImport { path, name } => Some(format!("Use `{path}.{name}` instead")),
            _ => None,
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
        Expr::Attribute(attribute_expr @ ExprAttribute { attr, .. }) => {
            check_name(checker, expr, attr.range());
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
            checker.report_diagnostics(diagnostic_for_argument(
                arguments,
                "fail_stop",
                Some("fail_fast"),
            ));
            checker.report_diagnostics(diagnostic_for_argument(
                arguments,
                "schedule_interval",
                Some("schedule"),
            ));
            checker.report_diagnostics(diagnostic_for_argument(
                arguments,
                "timetable",
                Some("schedule"),
            ));
            // without replacement
            checker.report_diagnostics(diagnostic_for_argument(arguments, "default_view", None));
            checker.report_diagnostics(diagnostic_for_argument(arguments, "orientation", None));
            checker.report_diagnostics(diagnostic_for_argument(
                arguments,
                "sla_miss_callback",
                None,
            ));
        }
        _ => {
            if is_airflow_auth_manager(qualified_name.segments()) {
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
            } else if is_airflow_task_handler(qualified_name.segments()) {
                checker.report_diagnostics(diagnostic_for_argument(
                    arguments,
                    "filename_template",
                    None,
                ));
            } else if is_airflow_operator(qualified_name.segments()) {
                checker.report_diagnostics(diagnostic_for_argument(arguments, "sla", None));
                checker.report_diagnostics(diagnostic_for_argument(
                    arguments,
                    "task_concurrency",
                    Some("max_active_tis_per_dag"),
                ));
                match qualified_name.segments() {
                    ["airflow", .., "operators", "trigger_dagrun", "TriggerDagRunOperator"] => {
                        checker.report_diagnostics(diagnostic_for_argument(
                            arguments,
                            "execution_date",
                            Some("logical_date"),
                        ));
                    }
                    ["airflow", .., "operators", "datetime", "BranchDateTimeOperator"]
                    | ["airflow", .., "operators", "weekday", "DayOfWeekSensor" | "BranchDayOfWeekOperator"] =>
                    {
                        checker.report_diagnostics(diagnostic_for_argument(
                            arguments,
                            "use_task_execution_day",
                            Some("use_task_logical_date"),
                        ));
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
            "dataset_factories" => Replacement::Name("asset_factories"),
            "dataset_uri_handlers" => Replacement::Name("asset_uri_handlers"),
            "dataset_to_openlineage_converters" => {
                Replacement::Name("asset_to_openlineage_converters")
            }
            _ => return,
        },
        ["airflow", "lineage", "hook", "DatasetLineageInfo"] => match attr.as_str() {
            "dataset" => Replacement::Name("asset"),
            _ => return,
        },
        _ => return,
    };

    // Create the `Fix` first to avoid cloning `Replacement`.
    let fix = if let Replacement::Name(name) = replacement {
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
            "register_dataset_change" => Replacement::Name("register_asset_change"),
            "create_datasets" => Replacement::Name("create_assets"),
            "notify_dataset_created" => Replacement::Name("notify_asset_created"),
            "notify_dataset_changed" => Replacement::Name("notify_asset_changed"),
            "notify_dataset_alias_created" => Replacement::Name("notify_asset_alias_created"),
            _ => return,
        },
        ["airflow", "lineage", "hook", "HookLineageCollector"] => match attr.as_str() {
            "create_dataset" => Replacement::Name("create_asset"),
            "add_input_dataset" => Replacement::Name("add_input_asset"),
            "add_output_dataset" => Replacement::Name("add_output_asset"),
            "collected_datasets" => Replacement::Name("collected_assets"),
            _ => return,
        },
        ["airflow", "providers", "amazon", "auth_manager", "aws_auth_manager", "AwsAuthManager"] => {
            match attr.as_str() {
                "is_authorized_dataset" => Replacement::Name("is_authorized_asset"),
                _ => return,
            }
        }
        ["airflow", "providers_manager", "ProvidersManager"] => match attr.as_str() {
            "initialize_providers_dataset_uri_resources" => {
                Replacement::Name("initialize_providers_asset_uri_resources")
            }
            _ => return,
        },
        ["airflow", "secrets", "local_filesystem", "LocalFilesystemBackend"] => match attr.as_str()
        {
            "get_connections" => Replacement::Name("get_connection"),
            _ => return,
        },
        ["airflow", "datasets", ..] | ["airflow", "Dataset"] => match attr.as_str() {
            "iter_datasets" => Replacement::Name("iter_assets"),
            "iter_dataset_aliases" => Replacement::Name("iter_asset_aliases"),
            _ => return,
        },
        segments => {
            if is_airflow_secret_backend(segments) {
                match attr.as_str() {
                    "get_conn_uri" => Replacement::Name("get_conn_value"),
                    "get_connections" => Replacement::Name("get_connection"),
                    _ => return,
                }
            } else if is_airflow_hook(segments) {
                match attr.as_str() {
                    "get_connections" => Replacement::Name("get_connection"),
                    _ => return,
                }
            } else {
                return;
            }
        }
    };
    // Create the `Fix` first to avoid cloning `Replacement`.
    let fix = if let Replacement::Name(name) = replacement {
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
        ["airflow", "PY36" | "PY37" | "PY38" | "PY39" | "PY310" | "PY311" | "PY312"] => {
            Replacement::Name("sys.version_info")
        }

        // airflow.api_connexion.security
        ["airflow", "api_connexion", "security", "requires_access"] => {
            Replacement::Name("airflow.api_connexion.security.requires_access_*")
        }
        ["airflow", "api_connexion", "security", "requires_access_dataset"] => {
            Replacement::AutoImport {
                path: "airflow.api_connexion.security",
                name: "requires_access_asset",
            }
        }

        // airflow.auth.managers
        ["airflow", "auth", "managers", "models", "resource_details", "DatasetDetails"] => {
            Replacement::Name(
                "airflow.api_fastapi.auth.managers.models.resource_details.AssetDetails",
            )
        }
        ["airflow", "auth", "managers", "base_auth_manager", "is_authorized_dataset"] => {
            Replacement::Name(
                "airflow.api_fastapi.auth.managers.base_auth_manager.is_authorized_asset",
            )
        }

        // airflow.configuration
        ["airflow", "configuration", rest @ ..] => match &rest {
            ["get"] => Replacement::Name("airflow.configuration.conf.get"),
            ["getboolean"] => Replacement::Name("airflow.configuration.conf.getboolean"),
            ["getfloat"] => Replacement::Name("airflow.configuration.conf.getfloat"),
            ["getint"] => Replacement::Name("airflow.configuration.conf.getint"),
            ["has_option"] => Replacement::Name("airflow.configuration.conf.has_option"),
            ["remove_option"] => Replacement::Name("airflow.configuration.conf.remove_option"),
            ["as_dict"] => Replacement::Name("airflow.configuration.conf.as_dict"),
            ["set"] => Replacement::Name("airflow.configuration.conf.set"),
            _ => return,
        },

        // airflow.contrib.*
        ["airflow", "contrib", ..] => {
            Replacement::Message("The whole `airflow.contrib` module has been removed.")
        }

        // airflow.datasets
        ["airflow", "Dataset"] | ["airflow", "datasets", "Dataset"] => Replacement::AutoImport {
            path: "airflow.sdk",
            name: "Asset",
        },
        ["airflow", "datasets", rest @ ..] => match &rest {
            ["DatasetAliasEvent"] => Replacement::None,
            ["DatasetAlias"] => Replacement::Name("airflow.sdk.AssetAlias"),
            ["DatasetAll"] => Replacement::Name("airflow.sdk.AssetAll"),
            ["DatasetAny"] => Replacement::Name("airflow.sdk.AssetAny"),
            ["expand_alias_to_datasets"] => Replacement::Name("airflow.sdk.expand_alias_to_assets"),
            ["metadata", "Metadata"] => Replacement::Name("airflow.sdk.Metadata"),
            // airflow.datasets.manager
            ["manager", "DatasetManager"] => Replacement::Name("airflow.assets.AssetManager"),
            ["manager", "dataset_manager"] => {
                Replacement::Name("airflow.assets.manager.asset_manager")
            }
            ["manager", "resolve_dataset_manager"] => {
                Replacement::Name("airflow.assets.resolve_asset_manager")
            }
            _ => return,
        },

        // airflow.hooks
        ["airflow", "hooks", "base_hook", "BaseHook"] => {
            Replacement::Name("airflow.hooks.base.BaseHook")
        }

        // airflow.lineage.hook
        ["airflow", "lineage", "hook", "DatasetLineageInfo"] => {
            Replacement::Name("airflow.lineage.hook.AssetLineageInfo")
        }

        // airflow.listeners.spec
        ["airflow", "listeners", "spec", "dataset", rest @ ..] => match &rest {
            ["on_dataset_created"] => {
                Replacement::Name("airflow.listeners.spec.asset.on_asset_created")
            }
            ["on_dataset_changed"] => {
                Replacement::Name("airflow.listeners.spec.asset.on_asset_changed")
            }
            _ => return,
        },

        // airflow.metrics.validators
        ["airflow", "metrics", "validators", rest @ ..] => match &rest {
            ["AllowListValidator"] => {
                Replacement::Name("airflow.metrics.validators.PatternAllowListValidator")
            }
            ["BlockListValidator"] => {
                Replacement::Name("airflow.metrics.validators.PatternBlockListValidator")
            }
            _ => return,
        },

        // airflow.models.baseoperator
        ["airflow", "models", "baseoperator", "chain"] => Replacement::Name("airflow.sdk.chain"),
        ["airflow", "models", "baseoperator", "chain_linear"] => {
            Replacement::Name("airflow.sdk.chain_linear")
        }
        ["airflow", "models", "baseoperator", "cross_downstream"] => {
            Replacement::Name("airflow.sdk.cross_downstream")
        }
        ["airflow", "models", "baseoperatorlink", "BaseOperatorLink"] => {
            Replacement::Name("airflow.sdk.definitions.baseoperatorlink.BaseOperatorLink")
        }

        // airflow.notifications
        ["airflow", "notifications", "basenotifier", "BaseNotifier"] => {
            Replacement::Name("airflow.sdk.BaseNotifier")
        }

        // airflow.operators
        ["airflow", "operators", "subdag", ..] => {
            Replacement::Message("The whole `airflow.subdag` module has been removed.")
        }
        ["airflow", "operators", "branch_operator", "BaseBranchOperator"] => {
            Replacement::Name("airflow.operators.branch.BaseBranchOperator")
        }
        ["airflow", "operators", "dummy" | "dummy_operator", "EmptyOperator" | "DummyOperator"] => {
            Replacement::Name("airflow.operators.empty.EmptyOperator")
        }
        ["airflow", "operators", "email_operator", "EmailOperator"] => {
            Replacement::Name("airflow.operators.email.EmailOperator")
        }
        ["airflow", "operators", "dagrun_operator", "TriggerDagRunLink"] => {
            Replacement::Name("airflow.operators.trigger_dagrun.TriggerDagRunLink")
        }
        ["airflow", "operators", "dagrun_operator", "TriggerDagRunOperator"] => {
            Replacement::Name("airflow.operators.trigger_dagrun.TriggerDagRunOperator")
        }
        ["airflow", "operators", "python_operator", "BranchPythonOperator"] => {
            Replacement::Name("airflow.operators.python.BranchPythonOperator")
        }
        ["airflow", "operators", "python_operator", "PythonOperator"] => {
            Replacement::Name("airflow.operators.python.PythonOperator")
        }
        ["airflow", "operators", "python_operator", "PythonVirtualenvOperator"] => {
            Replacement::Name("airflow.operators.python.PythonVirtualenvOperator")
        }
        ["airflow", "operators", "python_operator", "ShortCircuitOperator"] => {
            Replacement::Name("airflow.operators.python.ShortCircuitOperator")
        }
        ["airflow", "operators", "latest_only_operator", "LatestOnlyOperator"] => {
            Replacement::Name("airflow.operators.latest_only.LatestOnlyOperator")
        }

        // airflow.secrets
        ["airflow", "secrets", "local_filesystem", "load_connections"] => {
            Replacement::Name("airflow.secrets.local_filesystem.load_connections_dict")
        }

        // airflow.security
        ["airflow", "security", "permissions", "RESOURCE_DATASET"] => {
            Replacement::Name("airflow.security.permissions.RESOURCE_ASSET")
        }

        // airflow.sensors
        ["airflow", "sensors", "base_sensor_operator", "BaseSensorOperator"] => {
            Replacement::Name("airflow.sdk.bases.sensor.BaseSensorOperator")
        }
        ["airflow", "sensors", "date_time_sensor", "DateTimeSensor"] => {
            Replacement::Name("airflow.sensors.date_time.DateTimeSensor")
        }
        ["airflow", "sensors", "external_task" | "external_task_sensor", "ExternalTaskMarker"] => {
            Replacement::Name("airflow.sensors.external_task.ExternalTaskMarker")
        }
        ["airflow", "sensors", "external_task" | "external_task_sensor", "ExternalTaskSensorLink"] => {
            Replacement::Name("airflow.sensors.external_task.ExternalDagLink")
        }
        ["airflow", "sensors", "external_task" | "external_task_sensor", "ExternalTaskSensor"] => {
            Replacement::Name("airflow.sensors.external_task.ExternalTaskSensor")
        }
        ["airflow", "sensors", "time_delta_sensor", "TimeDeltaSensor"] => {
            Replacement::Name("airflow.sensors.time_delta.TimeDeltaSensor")
        }

        // airflow.timetables
        ["airflow", "timetables", rest @ ..] => match &rest {
            ["datasets", "DatasetOrTimeSchedule"] => {
                Replacement::Name("airflow.timetables.assets.AssetOrTimeSchedule")
            }
            ["simple", "DatasetTriggeredTimetable"] => {
                Replacement::Name("airflow.timetables.simple.AssetTriggeredTimetable")
            }
            _ => return,
        },

        // airflow.triggers
        ["airflow", "triggers", "external_task", "TaskStateTrigger"] => Replacement::None,

        // airflow.utils
        ["airflow", "utils", rest @ ..] => match &rest {
            // airflow.utils.dag_cycle_tester
            ["dag_cycle_tester", "test_cycle"] => Replacement::None,

            // airflow.utils.dag_parsing_context
            ["dag_parsing_context", "get_parsing_context"] => {
                Replacement::Name("airflow.sdk.get_parsing_context")
            }

            // airflow.utils.db
            ["db", "create_session"] => Replacement::None,

            // airflow.utils.decorators
            ["decorators", "apply_defaults"] => Replacement::Message(
                "`apply_defaults` is now unconditionally done and can be safely removed.",
            ),

            // airflow.utils.dates
            ["dates", "date_range"] => Replacement::None,
            ["dates", "days_ago"] => Replacement::Name("pendulum.today('UTC').add(days=-N, ...)"),
            ["dates", "parse_execution_date" | "round_time" | "scale_time_units" | "infer_time_unit"] => {
                Replacement::None
            }

            // airflow.utils.file
            ["file", "TemporaryDirectory"] => Replacement::Name("tempfile.TemporaryDirectory"),
            ["file", "mkdirs"] => Replacement::Name("pathlib.Path({path}).mkdir"),

            // airflow.utils.helpers
            ["helpers", "chain"] => Replacement::Name("airflow.sdk.chain"),
            ["helpers", "cross_downstream"] => Replacement::Name("airflow.sdk.cross_downstream"),

            // airflow.utils.log.secrets_masker
            ["log", "secrets_masker"] => {
                Replacement::Name("airflow.sdk.execution_time.secrets_masker")
            }

            // airflow.utils.state
            ["state", "SHUTDOWN" | "terminating_states"] => Replacement::None,

            // airflow.utils.trigger_rule
            ["trigger_rule", "TriggerRule", "DUMMY" | "NONE_FAILED_OR_SKIPPED"] => {
                Replacement::None
            }
            _ => return,
        },

        // airflow.www
        ["airflow", "www", "auth", "has_access"] => {
            Replacement::Name("airflow.www.auth.has_access_*")
        }
        ["airflow", "www", "auth", "has_access_dataset"] => {
            Replacement::Name("airflow.www.auth.has_access_dataset.has_access_asset")
        }
        ["airflow", "www", "utils", "get_sensitive_variables_fields"] => {
            Replacement::Name("airflow.utils.log.secrets_masker.get_sensitive_variables_fields")
        }
        ["airflow", "www", "utils", "should_hide_value_for_key"] => {
            Replacement::Name("airflow.utils.log.secrets_masker.should_hide_value_for_key")
        }

        // airflow.providers.amazon
        ["airflow", "providers", "amazon", "aws", rest @ ..] => match &rest {
            ["datasets", "s3", "create_dataset"] => {
                Replacement::Name("airflow.providers.amazon.aws.assets.s3.create_asset")
            }
            ["datasets", "s3", "convert_dataset_to_openlineage"] => Replacement::Name(
                "airflow.providers.amazon.aws.assets.s3.convert_asset_to_openlineage",
            ),
            ["datasets", "s3", "sanitize_uri"] => {
                Replacement::Name("airflow.providers.amazon.aws.assets.s3.sanitize_uri")
            }
            ["auth_manager", "avp", "entities", "AvpEntities", "DATASET"] => Replacement::Name(
                "airflow.providers.amazon.aws.auth_manager.avp.entities.AvpEntities.ASSET",
            ),
            _ => return,
        },

        // airflow.providers.common.io
        ["airflow", "providers", "common", "io", rest @ ..] => match &rest {
            ["datasets", "file", "create_dataset"] => {
                Replacement::Name("airflow.providers.common.io.assets.file.create_asset")
            }
            ["datasets", "file", "convert_dataset_to_openlineage"] => Replacement::Name(
                "airflow.providers.common.io.assets.file.convert_asset_to_openlineage",
            ),
            ["datasets", "file", "sanitize_uri"] => {
                Replacement::Name("airflow.providers.common.io.assets.file.sanitize_uri")
            }
            _ => return,
        },

        // airflow.providers.fab
        ["airflow", "providers", "fab", "auth_manager", "fab_auth_manager", "is_authorized_dataset"] => {
            Replacement::Name(
                "airflow.providers.fab.auth_manager.fab_auth_manager.is_authorized_asset",
            )
        }

        // airflow.providers.google
        ["airflow", "providers", "google", rest @ ..] => match &rest {
            ["datasets", "bigquery", "create_dataset"] => {
                Replacement::Name("airflow.providers.google.assets.bigquery.create_asset")
            }
            ["datasets", "gcs", "create_dataset"] => {
                Replacement::Name("airflow.providers.google.assets.gcs.create_asset")
            }
            ["datasets", "gcs", "convert_dataset_to_openlineage"] => Replacement::Name(
                "airflow.providers.google.assets.gcs.convert_asset_to_openlineage",
            ),
            ["datasets", "gcs", "sanitize_uri"] => {
                Replacement::Name("airflow.providers.google.assets.gcs.sanitize_uri")
            }
            _ => return,
        },

        // airflow.providers.mysql
        ["airflow", "providers", "mysql", "datasets", "mysql", "sanitize_uri"] => {
            Replacement::Name("airflow.providers.mysql.assets.mysql.sanitize_uri")
        }

        // airflow.providers.postgres
        ["airflow", "providers", "postgres", "datasets", "postgres", "sanitize_uri"] => {
            Replacement::Name("airflow.providers.postgres.assets.postgres.sanitize_uri")
        }

        // airflow.providers.openlineage
        ["airflow", "providers", "openlineage", rest @ ..] => match &rest {
            ["utils", "utils", "DatasetInfo"] => {
                Replacement::Name("airflow.providers.openlineage.utils.utils.AssetInfo")
            }

            ["utils", "utils", "translate_airflow_dataset"] => Replacement::Name(
                "airflow.providers.openlineage.utils.utils.translate_airflow_asset",
            ),
            _ => return,
        },

        // airflow.providers.trino
        ["airflow", "providers", "trino", "datasets", "trino", "sanitize_uri"] => {
            Replacement::Name("airflow.providers.trino.assets.trino.sanitize_uri")
        }

        _ => return,
    };

    if is_guarded_by_try_except(expr, &replacement, semantic) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: qualified_name.to_string(),
            replacement: replacement.clone(),
        },
        range,
    );

    if let Replacement::AutoImport { path, name } = replacement {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from(path, name),
                expr.start(),
                checker.semantic(),
            )?;
            let replacement_edit = Edit::range_replacement(binding, range);
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
    arguments: &Arguments,
    deprecated: &str,
    replacement: Option<&'static str>,
) -> Option<Diagnostic> {
    let keyword = arguments.find_keyword(deprecated)?;
    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: deprecated.to_string(),
            replacement: match replacement {
                Some(name) => Replacement::Name(name),
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

    Some(diagnostic)
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

/// Check whether the symbol is coming from the `operators` builtin or provider module which ends
/// with `Operator`.
fn is_airflow_operator(segments: &[&str]) -> bool {
    is_airflow_builtin_or_provider(segments, "operators", "Operator")
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

/// Check whether the segments corresponding to the fully qualified name points to a symbol that's
/// either a builtin or coming from one of the providers in Airflow.
///
/// The pattern it looks for are:
/// - `airflow.providers.**.<module>.**.*<symbol_suffix>` for providers
/// - `airflow.<module>.**.*<symbol_suffix>` for builtins
///
/// where `**` is one or more segments separated by a dot, and `*` is one or more characters.
///
/// Examples for the above patterns:
/// - `airflow.providers.google.cloud.secrets.secret_manager.CloudSecretManagerBackend` (provider)
/// - `airflow.secrets.base_secrets.BaseSecretsBackend` (builtin)
fn is_airflow_builtin_or_provider(segments: &[&str], module: &str, symbol_suffix: &str) -> bool {
    match segments {
        ["airflow", "providers", rest @ ..] => {
            if let (Some(pos), Some(last_element)) =
                (rest.iter().position(|&s| s == module), rest.last())
            {
                // Check that the module is not the last element i.e., there's a symbol that's
                // being used from the `module` that ends with `symbol_suffix`.
                pos + 1 < rest.len() && last_element.ends_with(symbol_suffix)
            } else {
                false
            }
        }

        ["airflow", first, rest @ ..] => {
            if let Some(last) = rest.last() {
                *first == module && last.ends_with(symbol_suffix)
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
