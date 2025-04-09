use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::Replacement;
use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ExprContext, ExprName, StmtFunctionDef};
use ruff_python_semantic::Modules;
use ruff_python_semantic::ScopeKind;
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
