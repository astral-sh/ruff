use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{name::QualifiedName, Arguments, Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, Eq, PartialEq)]
enum Replacement {
    None,
    Name(String),
}

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
    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3Removal {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::None => format!("`{deprecated}` is removed in Airflow 3.0"),
            Replacement::Name(name) => {
                format!("`{deprecated}` is removed in Airflow 3.0; use `{name}` instead")
            }
        }
    }
}

fn diagnostic_for_argument(
    arguments: &Arguments,
    deprecated: &str,
    replacement: Option<&str>,
) -> Option<Diagnostic> {
    let keyword = arguments.find_keyword(deprecated)?;
    Some(Diagnostic::new(
        Airflow3Removal {
            deprecated: (*deprecated).to_string(),
            replacement: match replacement {
                Some(name) => Replacement::Name(name.to_owned()),
                None => Replacement::None,
            },
        },
        keyword
            .arg
            .as_ref()
            .map_or_else(|| keyword.range(), Ranged::range),
    ))
}

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
        }
        _ => {}
    };
}

fn removed_name(checker: &mut Checker, expr: &Expr, ranged: impl Ranged) {
    let result =
        checker
            .semantic()
            .resolve_qualified_name(expr)
            .and_then(|qualname| match qualname.segments() {
                ["airflow", "triggers", "external_task", "TaskStateTrigger"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "api_connexion", "security", "requires_access"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.api_connexion.security.requires_access_*".to_string(),
                    ),
                )),
                // airflow.configuration
                ["airflow", "configuration", "get"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.get".to_string()),
                )),
                ["airflow", "configuration", "getboolean"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.getboolean".to_string()),
                )),
                ["airflow", "configuration", "getfloat"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.getfloat".to_string()),
                )),
                ["airflow", "configuration", "getint"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.getint".to_string()),
                )),
                ["airflow", "configuration", "has_option"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.has_option".to_string()),
                )),
                ["airflow", "configuration", "remove_option"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.remove_option".to_string()),
                )),
                ["airflow", "configuration", "as_dict"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.as_dict".to_string()),
                )),
                ["airflow", "configuration", "set"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.configuration.conf.set".to_string()),
                )),
                // airflow.contrib.*
                ["airflow", "contrib", ..] => Some((qualname.to_string(), Replacement::None)),
                // airflow.metrics.validators
                ["airflow", "metrics", "validators", "AllowListValidator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.metrics.validators.PatternAllowListValidator".to_string(),
                    ),
                )),
                ["airflow", "metrics", "validators", "BlockListValidator"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.metrics.validators.PatternBlockListValidator".to_string(),
                    ),
                )),
                // airflow.operators
                ["airflow", "operators", "subdag", ..] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow.sensors.external_task.ExternalTaskSensorLink"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.external_task.ExternalDagLin".to_string()),
                )),
                ["airflow", "operators", "bash_operator", "BashOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.bash.BashOperator".to_string()),
                )),
                ["airflow", "operators", "branch_operator", "BaseBranchOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.branch.BaseBranchOperator".to_string()),
                )),
                ["airflow", "operators", " dummy", "EmptyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator".to_string()),
                )),
                ["airflow", "operators", "dummy", "DummyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator".to_string()),
                )),
                ["airflow", "operators", "dummy_operator", "EmptyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator".to_string()),
                )),
                ["airflow", "operators", "dummy_operator", "DummyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.empty.EmptyOperator".to_string()),
                )),
                ["airflow", "operators", "email_operator", "EmailOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.operators.email.EmailOperator".to_string()),
                )),
                ["airflow", "sensors", "base_sensor_operator", "BaseSensorOperator"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.base.BaseSensorOperator".to_string()),
                )),
                ["airflow", "sensors", "date_time_sensor", "DateTimeSensor"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.date_time.DateTimeSensor".to_string()),
                )),
                ["airflow", "sensors", "external_task_sensor", "ExternalTaskMarker"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.sensors.external_task.ExternalTaskMarker".to_string(),
                    ),
                )),
                ["airflow", "sensors", "external_task_sensor", "ExternalTaskSensor"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.sensors.external_task.ExternalTaskSensor".to_string(),
                    ),
                )),
                ["airflow", "sensors", "external_task_sensor", "ExternalTaskSensorLink"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.sensors.external_task.ExternalTaskSensorLink".to_string(),
                    ),
                )),
                ["airflow", "sensors", "time_delta_sensor", "TimeDeltaSensor"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.sensors.time_delta.TimeDeltaSensor".to_string()),
                )),
                // airflow.secrets
                ["airflow", "secrets", "local_filesystem", "load_connections"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.secrets.local_filesystem.load_connections_dict".to_string(),
                    ),
                )),
                ["airflow", "secrets", "local_filesystem", "get_connection"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.secrets.local_filesystem.load_connections_dict".to_string(),
                    ),
                )),
                // airflow.utils.dates
                ["airflow", "utils", "dates", "date_range"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.timetables.".to_string()),
                )),
                ["airflow", "utils", "dates", "days_ago"] => Some((
                    qualname.to_string(),
                    Replacement::Name("pendulum.today('UTC').add(days=-N, ...)".to_string()),
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
                    Replacement::Name("pendulum.today('UTC').add(days=-N, ...)".to_string()),
                )),
                // airflow.utils.helpers
                ["airflow", "utils", "helpers", "chain"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.models.baseoperator.chain".to_string()),
                )),
                ["airflow", "utils", "helpers", "cross_downstream"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.models.baseoperator.cross_downstream".to_string()),
                )),
                // airflow.utils.state
                ["airflow", "utils", "state", "SHUTDOWN"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "state", "terminating_states"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                // airflow.uilts
                ["airflow", "utils", "dag_cycle_tester", "test_cycle"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                ["airflow", "utils", "decorators", "apply_defaults"] => {
                    Some((qualname.to_string(), Replacement::None))
                }
                // airflow.www
                ["airflow", "www", "auth", "has_access"] => Some((
                    qualname.to_string(),
                    Replacement::Name("airflow.www.auth.has_access_*".to_string()),
                )),
                ["airflow", "www", "utils", "get_sensitive_variables_fields"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.utils.log.secrets_masker.get_sensitive_variables_fields"
                            .to_string(),
                    ),
                )),
                ["airflow", "www", "utils", "should_hide_value_for_key"] => Some((
                    qualname.to_string(),
                    Replacement::Name(
                        "airflow.utils.log.secrets_masker.should_hide_value_for_key".to_string(),
                    ),
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
        }
        Expr::Attribute(ExprAttribute { attr: ranged, .. }) => removed_name(checker, expr, ranged),
        ranged @ Expr::Name(_) => removed_name(checker, expr, ranged),
        _ => {}
    }
}
