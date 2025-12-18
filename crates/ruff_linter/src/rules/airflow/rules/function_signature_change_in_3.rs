use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::Replacement;
use crate::{Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::Modules;
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.13.0")]
pub(crate) struct Airflow3FunctionSignatureChange {
    deprecated: String,
    replacement: Replacement,
}

/// ## What it does
/// Checks for Airflow function calls that use positional arguments when the
/// function signature changed to keyword-only arguments in Airflow 3.0.
///
/// ## Why is this bad?
/// Airflow 3.0 changed certain function signatures to only accept keyword
/// arguments. Using positional arguments will cause runtime errors.
///
/// ## Example
/// ```python
/// from airflow.lineage.hook import HookLineageCollector
///
/// collector = HookLineageCollector()
/// # Using positional arguments (will fail in Airflow 3.0)
/// collector.create_asset("s3://bucket/key")
/// ```
///
/// Use instead:
/// ```python
/// from airflow.lineage.hook import HookLineageCollector
///
/// collector = HookLineageCollector()
/// # Using keyword arguments
/// collector.create_asset(uri="s3://bucket/key")
/// ```
impl Violation for Airflow3FunctionSignatureChange {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3FunctionSignatureChange {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::None
            | Replacement::AttrName(_)
            | Replacement::Message(_)
            | Replacement::AttrNameWithMessage {
                attr_name: _,
                message: _,
            }
            | Replacement::Rename { module: _, name: _ }
            | Replacement::SourceModuleMoved { module: _, name: _ } => {
                format!("`{deprecated}` only accepts keyword arguments in Airflow 3.0")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3FunctionSignatureChange { replacement, .. } = self;
        match replacement {
            Replacement::None => None,
            Replacement::AttrName(name) => Some(format!("Use `{name}` instead")),
            Replacement::AttrNameWithMessage { attr_name, message } => {
                Some(format!("Use `{attr_name}` instead; {message}"))
            }
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
pub(crate) fn airflow_3_keyword_args_only_function(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    if let Expr::Call(call_expr @ ExprCall { arguments, .. }) = expr {
        check_method(checker, call_expr, arguments);
    }
}

fn check_method(checker: &Checker, call_expr: &ExprCall, arguments: &Arguments) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = &*call_expr.func else {
        return;
    };

    let Some(qualname) = typing::resolve_assignment(value, checker.semantic()) else {
        return;
    };

    let replacement = match qualname.segments() {
        ["airflow", "lineage", "hook", "HookLineageCollector"] => match attr.as_str() {
            "create_dataset" => {
                if arguments.find_positional(0).is_some() {
                    Replacement::AttrNameWithMessage {
                        attr_name: "create_asset",
                        message: "Calling ``HookLineageCollector.create_asset`` with positional argument should raise an error",
                    }
                } else {
                    Replacement::AttrName("create_asset")
                }
            }
            "create_asset" => {
                if arguments.find_positional(0).is_some() {
                    Replacement::Message(
                        "Calling ``HookLineageCollector.create_asset`` with positional argument should raise an error",
                    )
                } else {
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
            deprecated: attr.to_string(),
            replacement,
        },
        attr.range(),
    );
    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
}
