use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall, Identifier};
use ruff_python_semantic::Modules;
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for Airflow function calls that will raise a runtime error in Airflow 3.0
/// due to function signature changes, such as functions that changed to accept only
/// keyword arguments, parameter reordering, or parameter type changes.
///
/// ## Why is this bad?
/// Airflow 3.0 introduces changes to function signatures. Code that
/// worked in Airflow 2.x will raise a runtime error if not updated in Airflow
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
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct Airflow3IncompatibleFunctionSignature {
    function_name: String,
    change_type: FunctionSignatureChangeType,
}

impl Violation for Airflow3IncompatibleFunctionSignature {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3IncompatibleFunctionSignature {
            function_name,
            change_type,
        } = self;
        match change_type {
            FunctionSignatureChangeType::KeywordOnly { .. }
            | FunctionSignatureChangeType::PositionalArgumentChange { .. } => {
                format!("`{function_name}` signature is changed in Airflow 3.0")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3IncompatibleFunctionSignature { change_type, .. } = self;
        match change_type {
            FunctionSignatureChangeType::KeywordOnly { message }
            | FunctionSignatureChangeType::PositionalArgumentChange { message } => {
                Some(message.to_string())
            }
        }
    }
}

/// AIR303
pub(crate) fn airflow_3_incompatible_function_signature(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return;
    };

    // Handle method calls on instances
    if let Expr::Attribute(ExprAttribute { attr, value, .. }) = func.as_ref() {
        // Resolve the qualified name: try variable assignments first, then fall back to direct
        // constructor calls.
        let qualified_name = typing::resolve_assignment(value, checker.semantic()).or_else(|| {
            value
                .as_call_expr()
                .and_then(|call| checker.semantic().resolve_qualified_name(&call.func))
        });

        if let Some(qualified_name) = qualified_name {
            check_method_arguments(checker, &qualified_name, attr, arguments);
        }
        return;
    }

    // Handle direct constructor calls
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(func) else {
        return;
    };

    check_constructor_arguments(checker, &qualified_name, arguments, func);
}

fn check_method_arguments(
    checker: &Checker,
    qualified_name: &QualifiedName,
    attr: &Identifier,
    arguments: &Arguments,
) {
    let has_positional_args =
        arguments.find_positional(0).is_some() || arguments.args.iter().any(Expr::is_starred_expr);

    if let ["airflow", "lineage", "hook", "HookLineageCollector"] = qualified_name.segments() {
        if attr.as_str() == "create_asset" && has_positional_args {
            checker.report_diagnostic(
                Airflow3IncompatibleFunctionSignature {
                    function_name: attr.to_string(),
                    change_type: FunctionSignatureChangeType::KeywordOnly {
                        message: "Pass positional arguments as keyword arguments (e.g., `create_asset(uri=...)`)",
                    },
                },
                attr.range(),
            );
        }
    }
}

fn check_constructor_arguments(
    checker: &Checker,
    qualified_name: &QualifiedName,
    arguments: &Arguments,
    func: &Expr,
) {
    if let ["airflow", "Dataset"]
    | ["airflow", "datasets", "Dataset"]
    | ["airflow", "sdk", "Asset"] = qualified_name.segments()
    {
        if let Some(second_arg) = arguments.find_positional(1) {
            if is_dict_expression(checker, second_arg) {
                let function_name = qualified_name.segments().last().unwrap_or(&"").to_string();
                checker.report_diagnostic(
                    Airflow3IncompatibleFunctionSignature {
                        function_name,
                        change_type: FunctionSignatureChangeType::PositionalArgumentChange {
                            message: "Use keyword argument `extra` instead of passing a dict as the second positional argument (e.g., `Asset(name=..., uri=..., extra=...)`)",
                        },
                    },
                    func.range(),
                );
            }
        }
    }
}

/// Check if an expression is a dictionary.
fn is_dict_expression(checker: &Checker, expr: &Expr) -> bool {
    match expr {
        Expr::Dict(_) => true,
        Expr::DictComp(_) => true,
        Expr::Call(call) => checker
            .semantic()
            .resolve_builtin_symbol(&call.func)
            .is_some_and(|name| name == "dict"),
        Expr::Name(name) => checker
            .semantic()
            .resolve_name(name)
            .map(|id| checker.semantic().binding(id))
            .is_some_and(|binding| typing::is_dict(binding, checker.semantic())),
        _ => false,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FunctionSignatureChangeType {
    /// Function signature changed to only accept keyword arguments.
    KeywordOnly { message: &'static str },
    /// Function signature changed to not accept certain positional arguments.
    PositionalArgumentChange { message: &'static str },
}
