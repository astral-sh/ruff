use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::{Replacement, is_airflow_builtin_or_provider};
use crate::rules::airflow::helpers::{
    generate_import_edit, generate_remove_and_runtime_import_edit, is_guarded_by_try_except,
};
use crate::{Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall, ExprName, name::QualifiedName};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for uses of deprecated Airflow functions and values that still have
/// a compatibility layer.
///
/// ## Why is this bad?
/// Airflow 3.0 removed various deprecated functions, members, and other
/// values. Some have more modern replacements. Others are considered too niche
/// and not worth continued maintenance in Airflow.
/// Even though these symbols still work fine on Airflow 3.0, they are expected to be removed in a future version.
/// Where available, users should replace the removed functionality with the new alternatives.
///
/// ## Example
/// ```python
/// from airflow import Dataset
///
///
/// Dataset(uri="test://test/")
/// ```
///
/// Use instead:
/// ```python
/// from airflow.sdk import Asset
///
///
/// Asset(uri="test://test/")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.13.0")]
pub(crate) struct Airflow3SuggestedUpdate {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow3SuggestedUpdate {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3SuggestedUpdate { deprecated, .. } = self;
        format!(
            "`{deprecated}` is removed in Airflow 3.0; It still works in Airflow 3.0 but is expected to be removed in a future version."
        )
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3SuggestedUpdate { replacement, .. } = self;
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
                "`{name}` has been moved to `{module}` since Airflow 3.0 (with apache-airflow-task-sdk>={version})."
            )),
            Replacement::SourceModuleMovedWithMessage {
                module,
                name,
                message,
                ..
            } => Some(format!(
                "`{name}` has been moved to `{module}` since Airflow 3.0. {message}"
            )),
        }
    }
}

/// AIR311
pub(crate) fn airflow_3_0_suggested_update_expr(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            if let Some(qualified_name) = checker.semantic().resolve_qualified_name(func) {
                check_call_arguments(checker, &qualified_name, arguments);
            }
        }
        Expr::Attribute(ExprAttribute { attr, .. }) => {
            check_name(checker, expr, attr.range());
        }
        Expr::Name(ExprName {
            id: _,
            ctx: _,
            range,
            node_index: _,
        }) => {
            check_name(checker, expr, *range);
        }
        _ => {}
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
    let range = keyword
        .arg
        .as_ref()
        .map_or_else(|| keyword.range(), Ranged::range);
    let mut diagnostic = checker.report_diagnostic(
        Airflow3SuggestedUpdate {
            deprecated: deprecated.to_string(),
            replacement: match replacement {
                Some(name) => Replacement::AttrName(name),
                None => Replacement::None,
            },
        },
        range,
    );

    if let Some(replacement) = replacement {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            replacement.to_string(),
            range,
        )));
    }
}
/// Check whether a removed Airflow argument is passed.
///
/// For example:
///
/// ```python
/// from airflow import DAG
///
/// DAG(sla="@daily")
/// ```
fn check_call_arguments(checker: &Checker, qualified_name: &QualifiedName, arguments: &Arguments) {
    match qualified_name.segments() {
        ["airflow", .., "DAG" | "dag"] => {
            diagnostic_for_argument(checker, arguments, "sla_miss_callback", None);
        }
        ["airflow", "timetables", "datasets", "DatasetOrTimeSchedule"] => {
            diagnostic_for_argument(checker, arguments, "datasets", Some("assets"));
        }
        segments => {
            if is_airflow_builtin_or_provider(segments, "operators", "Operator") {
                diagnostic_for_argument(checker, arguments, "sla", None);
            }
        }
    }
}

/// Check whether a removed Airflow name is used.
///
/// For example:
///
/// ```python
/// from airflow import Dataset
/// from airflow import datasets
///
/// # Accessing via attribute
/// datasets.Dataset()
///
/// # Or, directly
/// Dataset()
/// ```
fn check_name(checker: &Checker, expr: &Expr, range: TextRange) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        // airflow.datasets.metadata
        ["airflow", "datasets", "metadata", "Metadata"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: "Metadata".to_string(),
            version: "1.0.0",
        },
        // airflow.datasets
        ["airflow", "Dataset"] | ["airflow", "datasets", "Dataset"] => {
            Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "Asset".to_string(),
                version: "1.0.0",
            }
        }
        ["airflow", "datasets", rest] => match *rest {
            "DatasetAliasEvent" => Replacement::None,
            "DatasetAlias" => Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "AssetAlias".to_string(),
                version: "1.0.0",
            },
            "DatasetAll" => Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "AssetAll".to_string(),
                version: "1.0.0",
            },
            "DatasetAny" => Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "AssetAny".to_string(),
                version: "1.0.0",
            },
            "expand_alias_to_datasets" => Replacement::Rename {
                module: "airflow.models.asset",
                name: "expand_alias_to_assets",
            },
            _ => return,
        },

        // airflow.decorators
        [
            "airflow",
            "decorators",
            rest @ ("dag" | "task" | "task_group" | "setup" | "teardown"),
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: (*rest).to_string(),
            version: "1.0.0",
        },
        [
            "airflow",
            "decorators",
            "base",
            rest @ ("DecoratedMappedOperator"
            | "DecoratedOperator"
            | "TaskDecorator"
            | "get_unique_task_id"
            | "task_decorator_factory"),
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk.bases.decorator",
            name: (*rest).to_string(),
            version: "1.0.0",
        },

        // airflow.io
        ["airflow", "io", "path", "ObjectStoragePath"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: "ObjectStoragePath".to_string(),
            version: "1.0.0",
        },
        ["airflow", "io", "store", "attach"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk.io",
            name: "attach".to_string(),
            version: "1.0.0",
        },

        // airflow.models
        ["airflow", "models", rest @ ("Connection" | "Variable")] => {
            Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: (*rest).to_string(),
                version: "1.0.0",
            }
        }
        [
            "airflow",
            "models",
            ..,
            rest @ ("Param" | "ParamsDict" | "DagParam"),
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk.definitions.param",
            name: (*rest).to_string(),
            version: "1.0.0",
        },

        // airflow.models.baseoperator
        [
            "airflow",
            "models",
            "baseoperator",
            rest @ ("chain" | "chain_linear" | "cross_downstream"),
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: (*rest).to_string(),
            version: "1.0.0",
        },
        ["airflow", "models", "baseoperatorlink", "BaseOperatorLink"] => {
            Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "BaseOperatorLink".to_string(),
                version: "1.0.0",
            }
        }

        // airflow.model..DAG
        ["airflow", "models", "dag", "DAG"] | ["airflow", "models", "DAG"] | ["airflow", "DAG"] => {
            Replacement::SourceModuleMovedToSDK {
                module: "airflow.sdk",
                name: "DAG".to_string(),
                version: "1.0.0",
            }
        }

        // airflow.sensors.base
        [
            "airflow",
            "sensors",
            "base",
            rest @ ("BaseSensorOperator" | "PokeReturnValue"),
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: (*rest).to_string(),
            version: "1.0.0",
        },

        ["airflow", "sensors", "base", "poke_mode_only"] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk.bases.sensor",
            name: "poke_mode_only".to_string(),
            version: "1.0.0",
        },

        // airflow.timetables
        ["airflow", "timetables", "datasets", "DatasetOrTimeSchedule"] => Replacement::Rename {
            module: "airflow.timetables.assets",
            name: "AssetOrTimeSchedule",
        },

        // airflow.utils
        [
            "airflow",
            "utils",
            "dag_parsing_context",
            "get_parsing_context",
        ] => Replacement::SourceModuleMovedToSDK {
            module: "airflow.sdk",
            name: "get_parsing_context".to_string(),
            version: "1.0.0",
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
                Airflow3SuggestedUpdate {
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
    let mut diagnostic = checker.report_diagnostic(
        Airflow3SuggestedUpdate {
            deprecated: qualified_name.to_string(),
            replacement: replacement.clone(),
        },
        range,
    );
    if let Some(fix) = generate_import_edit(expr, checker, module, name, range)
        .or_else(|| generate_remove_and_runtime_import_edit(expr, checker, module, name))
    {
        diagnostic.set_fix(fix);
    }
}
