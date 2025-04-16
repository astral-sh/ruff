use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::airflow::helpers::{
    is_airflow_builtin_or_provider, is_guarded_by_try_except, Replacement,
};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{name::QualifiedName, Arguments, Expr, ExprAttribute, ExprCall, ExprName};
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
/// and not worth to be maintained in Airflow.
/// Even though these symbols still work fine on Airflow 3.0, they are expected to be removed in a future version.
/// The user is suggested to replace the original usage with the new ones.
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
pub(crate) struct Airflow3SuggestedUpdate {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow3SuggestedUpdate {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3SuggestedUpdate {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::None
            | Replacement::Name(_)
            | Replacement::AutoImport { module: _, name: _ }
            | Replacement::SourceModuleMoved { module: _, name: _ } => {
                format!(
                    "`{deprecated}` is removed in Airflow 3.0; \
                    It still works in Airflow 3.0 but is expected to be removed in a future version."
                )
            }
            Replacement::Message(message) => {
                format!(
                    "`{deprecated}` is removed in Airflow 3.0; \
                     It still works in Airflow 3.0 but is expected to be removed in a future version.; \
                    {message}"
                )
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3SuggestedUpdate { replacement, .. } = self;
        match replacement {
            Replacement::Name(name) => Some(format!("Use `{name}` instead")),
            Replacement::AutoImport { module, name } => {
                Some(format!("Use `{module}.{name}` instead"))
            }
            Replacement::SourceModuleMoved { module, name } => {
                Some(format!("Use `{module}.{name}` instead"))
            }
            _ => None,
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
        }) => {
            check_name(checker, expr, *range);
        }
        _ => {}
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
        Airflow3SuggestedUpdate {
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
            checker.report_diagnostics(diagnostic_for_argument(
                arguments,
                "sla_miss_callback",
                None,
            ));
        }
        segments => {
            if is_airflow_builtin_or_provider(segments, "operators", "Operator") {
                checker.report_diagnostics(diagnostic_for_argument(arguments, "sla", None));
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
        ["airflow", "datasets", "metadata", "Metadata"] => {
            Replacement::Name("airflow.sdk.Metadata")
        }
        // airflow.datasets
        ["airflow", "Dataset"] | ["airflow", "datasets", "Dataset"] => Replacement::AutoImport {
            module: "airflow.sdk",
            name: "Asset",
        },
        ["airflow", "datasets", rest] => match *rest {
            "DatasetAliasEvent" => Replacement::None,
            "DatasetAlias" => Replacement::Name("airflow.sdk.AssetAlias"),
            "DatasetAll" => Replacement::Name("airflow.sdk.AssetAll"),
            "DatasetAny" => Replacement::Name("airflow.sdk.AssetAny"),
            "expand_alias_to_datasets" => Replacement::Name("airflow.sdk.expand_alias_to_assets"),
            _ => return,
        },

        // airflow.decorators
        ["airflow", "decorators", rest @ ("dag" | "task" | "task_group" | "setup" | "teardown")] => {
            Replacement::SourceModuleMoved {
                module: "airflow.sdk",
                name: (*rest).to_string(),
            }
        }

        // airflow.io
        ["airflow", "io", "path", "ObjectStoragePath"] => Replacement::SourceModuleMoved {
            module: "airflow.sdk",
            name: "ObjectStoragePath".to_string(),
        },
        ["airflow", "io", "storage", "attach"] => Replacement::SourceModuleMoved {
            module: "airflow.sdk.io",
            name: "attach".to_string(),
        },

        // airflow.models.baseoperator
        ["airflow", "models", "baseoperator", rest] => match *rest {
            "chain" | "chain_linear" | "cross_downstream" => Replacement::SourceModuleMoved {
                module: "airflow.sdk",
                name: (*rest).to_string(),
            },
            "BaseOperatorLink" => {
                Replacement::Name("airflow.sdk.definitions.baseoperatorlink.BaseOperatorLink")
            }
            _ => return,
        },
        // airflow.model..DAG
        ["airflow", "models", .., "DAG"] => Replacement::SourceModuleMoved {
            module: "airflow.sdk",
            name: "DAG".to_string(),
        },
        // airflow.timetables
        ["airflow", "timetables", "datasets", "DatasetOrTimeSchedule"] => {
            Replacement::Name("airflow.timetables.assets.AssetOrTimeSchedule")
        }
        // airflow.utils
        ["airflow", "utils", "dag_parsing_context", "get_parsing_context"] => {
            Replacement::Name("airflow.sdk.get_parsing_context")
        }

        _ => return,
    };

    if is_guarded_by_try_except(expr, &replacement, semantic) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        Airflow3SuggestedUpdate {
            deprecated: qualified_name.to_string(),
            replacement: replacement.clone(),
        },
        range,
    );

    if let Replacement::AutoImport { module, name } = replacement {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from(module, name),
                expr.start(),
                checker.semantic(),
            )?;
            let replacement_edit = Edit::range_replacement(binding, range);
            Ok(Fix::safe_edits(import_edit, [replacement_edit]))
        });
    }

    checker.report_diagnostic(diagnostic);
}
