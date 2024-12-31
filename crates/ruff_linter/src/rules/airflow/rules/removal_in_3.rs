use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    name::QualifiedName, Arguments, Expr, ExprAttribute, ExprCall, ExprContext, ExprName,
    StmtClassDef,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::Modules;
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3Removal { replacement, .. } = self;
        if let Replacement::Name(name) = replacement {
            Some(format!("Use `{name}` instead"))
        } else {
            None
        }
    }
}

/// AIR302
pub(crate) fn removed_in_3(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            if let Some(qualname) = checker.semantic().resolve_qualified_name(func) {
                removed_argument(checker, &qualname, arguments);
            };

            removed_method(checker, expr);
        }
        Expr::Attribute(ExprAttribute { attr: ranged, .. }) => {
            removed_name(checker, expr, ranged);
            removed_class_attribute(checker, expr);
        }
        ranged @ Expr::Name(ExprName { id, ctx, .. }) => {
            removed_name(checker, expr, ranged);
            if ctx == &ExprContext::Store {
                if let ScopeKind::Class(class_def) = &checker.semantic().current_scope().kind {
                    removed_airflow_plugin_extension(checker, expr, id, class_def);
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Replacement {
    None,
    Name(&'static str),
    Message(&'static str),
}

// Check whether a removed Airflow argument is passed.
//
// Example:
//
// from airflow import DAG
// DAG(schedule_interval="@daily")
fn removed_argument(checker: &mut Checker, qualname: &QualifiedName, arguments: &Arguments) {
    #[allow(clippy::single_match)]
    match qualname.segments() {
        ["airflow", .., "DAG" | "dag"] => {
            checker.diagnostics.extend(diagnostic_for_argument(
                arguments,
                "schedule_interval",
                Some("schedule"),
            ));
            checker.diagnostics.extend(diagnostic_for_argument(
                arguments,
                "timetable",
                Some("schedule"),
            ));
            checker.diagnostics.extend(diagnostic_for_argument(
                arguments,
                "sla_miss_callback",
                None::<&str>,
            ));
        }
        _ => {
            if is_airflow_auth_manager(qualname.segments()) {
                if !arguments.is_empty() {
                    checker.diagnostics.push(Diagnostic::new(
                        Airflow3Removal {
                            // deprecated: (*arguments).to_string(),
                            deprecated: "appbuilder".to_string(),
                            replacement: Replacement::Message(
                                "The constructor takes no parameter now.",
                            ),
                        },
                        arguments.range(),
                    ));
                }
            } else if is_airflow_task_handler(qualname.segments()) {
                checker.diagnostics.extend(diagnostic_for_argument(
                    arguments,
                    "filename_template",
                    None::<&str>,
                ));
            } else if is_airflow_operator(qualname.segments()) {
                checker
                    .diagnostics
                    .extend(diagnostic_for_argument(arguments, "sla", None::<&str>));
                checker.diagnostics.extend(diagnostic_for_argument(
                    arguments,
                    "task_concurrency",
                    Some("max_active_tis_per_dag"),
                ));
                match qualname.segments() {
                    ["airflow", .., "operators", "trigger_dagrun", "TriggerDagRunOperator"] => {
                        checker.diagnostics.extend(diagnostic_for_argument(
                            arguments,
                            "execution_date",
                            Some("logical_date"),
                        ));
                    }
                    ["airflow", .., "operators", "datetime", "BranchDateTimeOperator"] => {
                        checker.diagnostics.extend(diagnostic_for_argument(
                            arguments,
                            "use_task_execution_day",
                            Some("use_task_logical_date"),
                        ));
                    }
                    ["airflow", .., "operators", "weekday", "DayOfWeekSensor"] => {
                        checker.diagnostics.extend(diagnostic_for_argument(
                            arguments,
                            "use_task_execution_day",
                            Some("use_task_logical_date"),
                        ));
                    }
                    ["airflow", .., "operators", "weekday", "BranchDayOfWeekOperator"] => {
                        checker.diagnostics.extend(diagnostic_for_argument(
                            arguments,
                            "use_task_execution_day",
                            Some("use_task_logical_date"),
                        ));
                    }
                    _ => {}
                }
            }
        }
    };
}

// Check whether a removed Airflow class attribute (include property) is called.
//
// Example:
//
// from airflow.linesage.hook import DatasetLineageInfo
// info = DatasetLineageInfo()
// info.dataset
fn removed_class_attribute(checker: &mut Checker, expr: &Expr) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = expr else {
        return;
    };

    let Some(qualname) = typing::resolve_assignment(value, checker.semantic()) else {
        return;
    };

    let replacement = match *qualname.segments() {
        ["airflow", "providers_manager", "ProvidersManager"] => match attr.as_str() {
            "dataset_factories" => Some(Replacement::Name("asset_factories")),
            "dataset_uri_handlers" => Some(Replacement::Name("asset_uri_handlers")),
            "dataset_to_openlineage_converters" => {
                Some(Replacement::Name("asset_to_openlineage_converters"))
            }
            &_ => None,
        },
        ["airflow", "lineage", "hook"] => match attr.as_str() {
            "dataset" => Some(Replacement::Name("asset")),
            &_ => None,
        },
        _ => None,
    };
    if let Some(replacement) = replacement {
        checker.diagnostics.push(Diagnostic::new(
            Airflow3Removal {
                deprecated: attr.to_string(),
                replacement,
            },
            attr.range(),
        ));
    }
}

// Check whether a removed Airflow class method is called.
//
// Example:
//
// from airflow.datasets.manager import DatasetManager
//
// manager = DatasetManager()
// manger.register_datsaet_change()
fn removed_method(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ExprCall { func, .. }) = expr else {
        return;
    };

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = &**func else {
        return;
    };

    let Some(qualname) = typing::resolve_assignment(value, checker.semantic()) else {
        return;
    };

    let replacement = match *qualname.segments() {
        ["airflow", "datasets", "manager", "DatasetManager"] => match attr.as_str() {
            "register_dataset_change" => Some(Replacement::Name("register_asset_change")),
            "create_datasets" => Some(Replacement::Name("create_assets")),
            "notify_dataset_created" => Some(Replacement::Name("notify_asset_created")),
            "notify_dataset_changed" => Some(Replacement::Name("notify_asset_changed")),
            "notify_dataset_alias_created" => Some(Replacement::Name("notify_asset_alias_created")),
            &_ => None,
        },
        ["airflow", "lineage", "hook", "HookLineageCollector"] => match attr.as_str() {
            "create_dataset" => Some(Replacement::Name("create_asset")),
            "add_input_dataset" => Some(Replacement::Name("add_input_asset")),
            "add_output_dataset" => Some(Replacement::Name("add_output_asset")),
            "collected_datasets" => Some(Replacement::Name("collected_assets")),
            &_ => None,
        },
        ["airflow", "providers", "amazon", "auth_manager", "aws_auth_manager", "AwsAuthManager"] => {
            match attr.as_str() {
                "is_authorized_dataset" => Some(Replacement::Name("is_authorized_asset")),
                &_ => None,
            }
        }
        ["airflow", "providers_manager", "ProvidersManager"] => match attr.as_str() {
            "initialize_providers_dataset_uri_resources" => Some(Replacement::Name(
                "initialize_providers_asset_uri_resources",
            )),
            &_ => None,
        },
        ["airflow", "datasets", ..] | ["airflow", "Dataset"] => match attr.as_str() {
            "iter_datasets" => Some(Replacement::Name("iter_assets")),
            "iter_dataset_aliases" => Some(Replacement::Name("iter_asset_aliases")),
            &_ => None,
        },
        _ => {
            if is_airflow_secret_backend(qualname.segments()) {
                match attr.as_str() {
                    "get_conn_uri" => Some(Replacement::Name("get_conn_value")),
                    "get_connections" => Some(Replacement::Name("get_connection")),
                    &_ => None,
                }
            } else if is_airflow_hook(qualname.segments()) {
                match attr.as_str() {
                    "get_connections" => Some(Replacement::Name("get_connection")),
                    &_ => None,
                }
            } else {
                None
            }
        }
    };
    if let Some(replacement) = replacement {
        checker.diagnostics.push(Diagnostic::new(
            Airflow3Removal {
                deprecated: attr.to_string(),
                replacement,
            },
            attr.range(),
        ));
    }
}

// Check whether a removed Airflow name is used.
//
// Example:
//
// from airflow.operators.subdag import SubDagOperator
fn removed_name(checker: &mut Checker, expr: &Expr, ranged: impl Ranged) {
    let result =
        checker
            .semantic()
            .resolve_qualified_name(expr)
            .and_then(|qualname| match qualname.segments() {
                ["airflow", "api_connexion", "security", "requires_access"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.api_connexion.security.requires_access_*",
                    ),
                )),
                ["airflow", "api_connexion", "security", "requires_access_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.api_connexion.security.requires_access_asset"),
                )),
                ["airflow", "triggers", "external_task", "TaskStateTrigger"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "security", "permissions", "RESOURCE_DATASET"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.security.permissions.RESOURCE_ASSET"),
                )),
                // airflow.PY\d{1,2}
                ["airflow", "PY36"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                ["airflow", "PY37"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                ["airflow", "PY38"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                ["airflow", "PY39"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                ["airflow", "PY310"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                ["airflow", "PY311"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                ["airflow", "PY312"] => Some((
                    qualname.to_string(),
                    Replacement::Name("sys.version_info"),
                )),
                // airflow.configuration
                ["airflow", "configuration", "get"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.get"),
                )),
                ["airflow", "configuration", "getboolean"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.getboolean"),
                )),
                ["airflow", "configuration", "getfloat"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.getfloat"),
                )),
                ["airflow", "configuration", "getint"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.getint"),
                )),
                ["airflow", "configuration", "has_option"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.has_option"),
                )),
                ["airflow", "configuration", "remove_option"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.remove_option"),
                )),
                ["airflow", "configuration", "as_dict"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.as_dict"),
                )),
                ["airflow", "configuration", "set"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.set"),
                )),
                // airflow.auth.managers
                ["airflow", "auth", "managers", "models", "resource_details", "DatasetDetails"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.auth.managers.models.resource_details.AssetDetails"),
                )),
                ["airflow", "auth", "managers", "base_auth_manager", "is_authorized_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.auth.managers.base_auth_manager.is_authorized_asset"),
                )),
                // airflow.contrib.*
                ["airflow", "contrib", ..] => Some((qualname.to_string(),
                    Replacement::Message(
                        "The whole `airflow.contrib` module has been removed.",
                    ),
                )),
                // airflow.metrics.validators
                ["airflow", "metrics", "validators", "AllowListValidator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.metrics.validators.PatternAllowListValidator",
                    ),
                )),
                ["airflow", "metrics", "validators", "BlockListValidator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.metrics.validators.PatternBlockListValidator",
                    ),
                )),
                // airflow.datasets
                ["airflow", "Dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.Asset"),
                )),
                ["airflow", "datasets", "DatasetAliasEvent"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "datasets", "Dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.Asset"),
                )),
                ["airflow", "datasets", "DatasetAlias"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.AssetAlias"),
                )),
                ["airflow", "datasets", "DatasetAll"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.AssetAll"),
                )),
                ["airflow", "datasets", "DatasetAny"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.AssetAny"),
                )),
                ["airflow", "datasets", "expand_alias_to_datasets"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.expand_alias_to_assets"),
                )),
                ["airflow", "datasets", "metadata", "Metadata"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sdk.definitions.asset.metadata.Metadata"),
                )),
                // airflow.datasets.manager
                ["airflow", "datasets", "manager", "dataset_manager"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.assets.manager"),
                )),
                ["airflow", "datasets", "manager", "resolve_dataset_manager"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.assets.resolve_asset_manager"),
                )),
                ["airflow", "datasets.manager", "DatasetManager"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.assets.AssetManager"),
                )),
                // airflow.listeners.spec
                ["airflow", "listeners", "spec", "dataset", "on_dataset_created"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.listeners.spec.asset.on_asset_created"),
                )),
                ["airflow", "listeners", "spec", "dataset", "on_dataset_changed"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.listeners.spec.asset.on_asset_changed"),
                )),
                // airflow.timetables
                ["airflow", "timetables", "datasets", "DatasetOrTimeSchedule"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.timetables.assets.AssetOrTimeSchedule"),
                )),
                ["airflow", "timetables", "simple", "DatasetTriggeredTimetable"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.timetables.simple.AssetTriggeredTimetable"),
                )),
                // airflow.lineage.hook
                ["airflow", "lineage", "hook", "DatasetLineageInfo"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.lineage.hook.AssetLineageInfo"),
                )),
                // airflow.hooks
                ["airflow", "hooks", "base_hook", "BaseHook"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.hooks.base.BaseHook"),
                )),
                // airflow.operators
                ["airflow", "operators", "subdag", ..] => {
                    Some((
                        qualname.to_string(),
                        Replacement::Message(
                            "The whole `airflow.subdag` module has been removed.",
                        ),
                    ))
                },
                ["airflow", "operators", "bash_operator", "BashOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.bash.BashOperator"),
                )),
                ["airflow", "operators", "branch_operator", "BaseBranchOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.branch.BaseBranchOperator"),
                )),
                ["airflow", "operators", " dummy", "EmptyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator"),
                )),
                ["airflow", "operators", "dummy", "DummyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator"),
                )),
                ["airflow", "operators", "dummy_operator", "EmptyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator"),
                )),
                ["airflow", "operators", "dummy_operator", "DummyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator"),
                )),
                ["airflow", "operators", "email_operator", "EmailOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.email.EmailOperator"),
                )),
                ["airflow", "operators", "dagrun_operator", "TriggerDagRunLink"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.operators.trigger_dagrun.TriggerDagRunLink",
                    ),
                )),
                ["airflow", "operators", "dagrun_operator", "TriggerDagRunOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.operators.trigger_dagrun.TriggerDagRunOperator",
                    ),
                )),
                ["airflow", "operators", "python_operator", "BranchPythonOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.operators.python.BranchPythonOperator",
                    ),
                )),
                ["airflow", "operators", "python_operator", "PythonOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.operators.python.PythonOperator",
                    ),
                )),
                ["airflow", "operators", "python_operator", "PythonVirtualenvOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.operators.python.PythonVirtualenvOperator",
                    ),
                )),
                ["airflow", "operators", "python_operator", "ShortCircuitOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.operators.python.ShortCircuitOperator",
                    ),
                )),
                ["airflow", "operators", "latest_only_operator", "LatestOnlyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "     airflow.operators.latest_only.LatestOnlyOperator",
                    ),
                )),
                // airflow.sensors
                ["airflow", "sensors", "external_task", "ExternalTaskSensorLink"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.external_task.ExternalDagLink"),
                )),
                ["airflow", "sensors", "base_sensor_operator", "BaseSensorOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.base.BaseSensorOperator"),
                )),
                ["airflow", "sensors", "date_time_sensor", "DateTimeSensor"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.date_time.DateTimeSensor"),
                )),
                ["airflow", "sensors", "external_task_sensor", "ExternalTaskMarker"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.sensors.external_task.ExternalTaskMarker",
                    ),
                )),
                ["airflow", "sensors", "external_task_sensor", "ExternalTaskSensor"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.sensors.external_task.ExternalTaskSensor",
                    ),
                )),
                ["airflow", "sensors", "external_task_sensor", "ExternalTaskSensorLink"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.sensors.external_task.ExternalDagLink",
                    ),
                )),
                ["airflow", "sensors", "time_delta_sensor", "TimeDeltaSensor"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.time_delta.TimeDeltaSensor"),
                )),
                // airflow.secrets
                ["airflow", "secrets", "local_filesystem", "load_connections"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.secrets.local_filesystem.load_connections_dict",
                    ),
                )),
                ["airflow", "secrets", "local_filesystem", "get_connection"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.secrets.local_filesystem.load_connections_dict",
                    ),
                )),
                // airflow.utils.dates
                ["airflow", "utils", "dates", "date_range"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.timetables."),
                )),
                ["airflow", "utils", "dates", "days_ago"] => Some((
                    qualname.to_string(),
                    Replacement::Name("pendulum.today('UTC').add(days=-N, ...)"),
                )),
                ["airflow", "utils", "dates", "parse_execution_date"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "dates", "round_time"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "dates", "scale_time_units"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "dates", "infer_time_unit"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                // airflow.utils.file
                ["airflow", "utils", "file", "TemporaryDirectory"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "file", "mkdirs"] => Some((
                    qualname.to_string(),
                    Replacement::Name("pendulum.today('UTC').add(days=-N, ...)"),
                )),
                // airflow.utils.helpers
                ["airflow", "utils", "helpers", "chain"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.models.baseoperator.chain"),
                )),
                ["airflow", "utils", "helpers", "cross_downstream"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.models.baseoperator.cross_downstream"),
                )),
                // airflow.utils.state
                ["airflow", "utils", "state", "SHUTDOWN"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "state", "terminating_states"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                // airflow.utils.trigger_rule
                ["airflow", "utils", "trigger_rule", "TriggerRule", "DUMMY"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "trigger_rule", "TriggerRule", "NONE_FAILED_OR_SKIPPED"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                // airflow.uilts
                ["airflow", "utils", "dag_cycle_tester", "test_cycle"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "decorators", "apply_defaults"] => Some((
                    qualname.to_string(),
                    Replacement::Message(
                        "`apply_defaults` is now unconditionally done and can be safely removed.",
                    ),
                )),
                // airflow.www
                ["airflow", "www", "auth", "has_access"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.www.auth.has_access_*"),
                )),
                ["airflow", "www", "auth", "has_access_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.www.auth.has_access_dataset.has_access_asset"),
                )),
                ["airflow", "www", "utils", "get_sensitive_variables_fields"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.utils.log.secrets_masker.get_sensitive_variables_fields",
                    ),
                )),
                ["airflow", "www", "utils", "should_hide_value_for_key"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.utils.log.secrets_masker.should_hide_value_for_key",
                    ),
                )),
                // airflow.providers.amazon
                ["airflow", "providers", "amazon", "aws", "datasets", "s3", "create_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.amazon.aws.assets.s3.create_asset"),
                )),
                ["airflow", "providers", "amazon", "aws", "datasets", "s3", "convert_dataset_to_openlineage"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.amazon.aws.assets.s3.convert_asset_to_openlineage"),
                )),
                ["airflow", "providers", "amazon", "aws", "datasets", "s3", "sanitize_uri"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.amazon.aws.assets.s3.sanitize_uri"),
                )),
                ["airflow", "providers", "amazon", "auth_manager", "avp", "entities", "AvpEntities", "DATASET"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.amazon.auth_manager.avp.entities.AvpEntities.ASSET"),
                )),
                // airflow.providers.common.io
                ["airflow", "providers", "common", "io", "datasets", "file", "create_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.common.io.assets.file.create_asset"),
                )),
                ["airflow", "providers", "common", "io", "datasets", "file", "convert_dataset_to_openlineage"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.common.io.assets.file.convert_asset_to_openlineage"),
                )),
                ["airflow", "providers", "common", "io", "datasets", "file", "sanitize_uri"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.common.io.assets.file.sanitize_uri"),
                )),
                // airflow.providers.fab
                ["airflow", "providers", "fab", "auth_manager", "fab_auth_manager", "is_authorized_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.fab.auth_manager.fab_auth_manager.is_authorized_asset"),
                )),
                // airflow.providers.google
                ["airflow", "providers", "google", "datasets", "bigquery", "create_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.google.assets.bigquery.create_asset"),
                )),
                ["airflow", "providers", "google", "datasets", "gcs", "create_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.google.assets.gcs.create_asset"),
                )),
                ["airflow", "providers", "google", "datasets", "gcs", "convert_dataset_to_openlineage"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.google.assets.gcs.convert_asset_to_openlineage"),
                )),
                ["airflow", "providers", "google", "datasets", "gcs", "sanitize_uri"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.google.assets.gcs.sanitize_uri"),
                )),
                // airflow.providers.mysql
                ["airflow", "providers", "mysql", "datasets", "mysql", "sanitize_uri"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.mysql.assets.mysql.sanitize_uri"),
                )),
                // airflow.providers.postgres
                ["airflow", "providers", "postgres", "datasets", "postgres", "sanitize_uri"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.postgres.assets.postgres.sanitize_uri"),
                )),
                // airflow.providers.openlineage
                ["airflow", "providers", "openlineage", "utils", "utils", "DatasetInfo"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.openlineage.utils.utils.AssetInfo"),
                )),
                ["airflow", "providers", "openlineage", "utils", "utils", "translate_airflow_dataset"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.openlineage.utils.utils.translate_airflow_asset"),
                )),
                // airflow.providers.trino
                ["airflow", "providers", "trino", "datasets", "trino", "sanitize_uri"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.providers.trino.assets.trino.sanitize_uri"),
                )),
                _ => None,
            });
    if let Some((deprecated, replacement)) = result {
        checker.diagnostics.push(Diagnostic::new(
            Airflow3Removal {
                deprecated,
                replacement,
            },
            ranged.range(),
        ));
    }
}

// Check whether a customized Airflow plugin contains removed extensions.
//
// Example:
//
// class CustomizePlugin(AirflowPlugin)
//     executors = "some.third.party.executor"
fn removed_airflow_plugin_extension(
    checker: &mut Checker,
    expr: &Expr,
    name: &str,
    class_def: &StmtClassDef,
) {
    if matches!(name, "executors" | "operators" | "sensors" | "hooks") {
        if class_def.bases().iter().any(|expr| {
            checker
                .semantic()
                .resolve_qualified_name(expr)
                .is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["airflow", "plugins_manager", "AirflowPlugin"]
                    )
                })
        }) {
            checker.diagnostics.push(Diagnostic::new(
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

fn diagnostic_for_argument(
    arguments: &Arguments,
    deprecated: &str,
    replacement: Option<&'static str>,
) -> Option<Diagnostic> {
    let keyword = arguments.find_keyword(deprecated)?;
    let mut diagnostic = Diagnostic::new(
        Airflow3Removal {
            deprecated: (*deprecated).to_string(),
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
