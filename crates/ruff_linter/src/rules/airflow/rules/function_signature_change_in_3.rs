use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::Replacement;
use crate::{Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::Modules;
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for Airflow function calls that will raise a runtime error in Airflow 3.0
/// due to function signature changes, such as functions that changed to accept only
/// keyword arguments, parameter reordering, or parameter type changes.
///
/// ## Why is this bad?
/// Airflow 3.0 might introduce changes to function signatures. Code that
/// worked in Airflow 2.x may raise a runtime error if not updated in Airflow
/// 3.0.
///
/// ## Example
/// ```python
/// from airflow.lineage.hook import HookLineageCollector
///
/// collector = HookLineageCollector()
/// # Passing positional arguments will raise a runtime error in Airflow 3.0
/// collector.create_asset("s3://bucket/key")
/// ```
///
/// Use instead:
/// ```python
/// from airflow.lineage.hook import HookLineageCollector
///
/// collector = HookLineageCollector()
/// # Passing arguments as keyword arguments instead of positional arguments
/// collector.create_asset(uri="s3://bucket/key")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.14.11")]
pub(crate) struct Airflow3FunctionSignatureChange {
    function_def: String,
    replacement: Replacement,
}

impl Violation for Airflow3FunctionSignatureChange {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3FunctionSignatureChange {
            function_def,
            replacement,
        } = self;
        match replacement {
            Replacement::None
            | Replacement::AttrName(_)
            | Replacement::Message(_)
            | Replacement::Rename { module: _, name: _ }
            | Replacement::SourceModuleMoved { module: _, name: _ } => {
                format!("`{function_def}` only accepts keyword arguments in Airflow 3.0")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3FunctionSignatureChange { replacement, .. } = self;
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

/// AIR303
pub(crate) fn airflow_3_function_signature_change_expr(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    airflow_3_keyword_args_only_function(checker, expr);
}

/// Check for functions that changed to only accept keyword arguments
fn airflow_3_keyword_args_only_function(checker: &Checker, expr: &Expr) {
    if let Expr::Call(call_expr @ ExprCall { arguments, .. }) = expr {
        check_keyword_only_method(checker, call_expr, arguments);
    }
}

fn check_keyword_only_method(checker: &Checker, call_expr: &ExprCall, arguments: &Arguments) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = &*call_expr.func else {
        return;
    };

    let Some(qualname) = typing::resolve_assignment(value, checker.semantic()) else {
        return;
    };

    let replacement = match qualname.segments() {
        ["airflow", "lineage", "hook", "HookLineageCollector"] => match attr.as_str() {
            "create_asset" => {
                if arguments.find_positional(0).is_some() {
                    Replacement::Message(
                        "Pass positional arguments as keyword arguments (e.g., `create_asset(uri=...)`)",
                    )
                } else {
                    // No positional args, no violation
                    return;
                }
            }
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

    let mut diagnostic = checker.report_diagnostic(
        Airflow3FunctionSignatureChange {
            function_def: attr.to_string(),
            replacement,
        },
        attr.range(),
    );
    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
}
