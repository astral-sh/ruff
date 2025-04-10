use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::is_airflow_operator;
use crate::rules::airflow::helpers::Replacement;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{name::QualifiedName, Arguments, Expr, ExprCall};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

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
                format!("`{deprecated}` is removed in Airflow 3.0")
            }
            Replacement::Message(message) => {
                format!("`{deprecated}` is removed in Airflow 3.0; {message}")
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
        _ => {
            if is_airflow_operator(qualified_name.segments()) {
                checker.report_diagnostics(diagnostic_for_argument(arguments, "sla", None));
            }
        }
    }
}
