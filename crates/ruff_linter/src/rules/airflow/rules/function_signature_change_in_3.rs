use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::{FunctionSignatureChange, is_method_in_subclass};
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall, Identifier, StmtFunctionDef};
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
    change: FunctionSignatureChange,
}

impl Violation for Airflow3IncompatibleFunctionSignature {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3IncompatibleFunctionSignature { function_name, .. } = self;
        format!("`{function_name}` signature is changed in Airflow 3.0")
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3IncompatibleFunctionSignature { change, .. } = self;
        let FunctionSignatureChange::Message(message) = change;
        Some((*message).to_string())
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

/// AIR303
pub(crate) fn airflow_3_incompatible_method_signature_def(
    checker: &Checker,
    function_def: &StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // Check for deprecated get_link signature in BaseOperatorLink subclasses
    if is_method_in_subclass(
        function_def,
        checker.semantic(),
        "get_link",
        |qualified_name| {
            matches!(
                qualified_name.segments(),
                ["airflow", "models" | "sdk", .., "BaseOperatorLink"]
            )
        },
    ) {
        let parameters = &function_def.parameters;
        let positional_count = parameters.posonlyargs.len() + parameters.args.len();

        let is_valid_signature = match positional_count {
            // check valid signature `def get_link(self, operator, *, ti_key)`
            2 => parameters
                .kwonlyargs
                .iter()
                .any(|p| p.name().as_str() == "ti_key"),
            // check valid signature `def get_link(self, operator, ti_key)`
            3 => parameters
                .posonlyargs
                .iter()
                .chain(parameters.args.iter())
                .nth(2)
                .is_some_and(|p| p.name().as_str() == "ti_key"),
            _ => false,
        };

        if !is_valid_signature {
            checker.report_diagnostic(
                Airflow3IncompatibleFunctionSignature {
                    function_name: "get_link".to_string(),
                    change: FunctionSignatureChange::Message(
                        "Use `def get_link(self, operator, *, ti_key)` or `def get_link(self, operator, ti_key)` as the method signature.",
                    ),
                },
                function_def.name.range(),
            );
        }
    }
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
                    change: FunctionSignatureChange::Message(
                        "Pass positional arguments as keyword arguments (e.g., `create_asset(uri=...)`)",
                    ),
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
                        change: FunctionSignatureChange::Message(
                            "Use keyword argument `extra` instead of passing a dict as the second positional argument (e.g., `Asset(name=..., uri=..., extra=...)`)",
                        ),
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
