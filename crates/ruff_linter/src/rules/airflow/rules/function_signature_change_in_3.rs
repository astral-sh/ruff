use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::{FunctionSignatureChange, is_method_in_subclass};
use crate::{Edit, Fix, FixAvailability, Violation};
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3IncompatibleFunctionSignature { function_name, .. } = self;
        format!("`{function_name}` signature is changed in Airflow 3.0")
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3IncompatibleFunctionSignature { change, .. } = self;
        match change {
            FunctionSignatureChange::Message(message) => Some((*message).to_string()),
            FunctionSignatureChange::ArgName { old, new } => {
                Some(format!("Use `{new}` instead of `{old}`"))
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
        // Resolve the qualified name:
        // 1. Variable assignments (e.g., `var = SomeClass(); var.method()`)
        // 2. Direct constructor calls (e.g., `SomeClass().method()`)
        // 3. Class/static method calls (e.g., `Variable.get()`)
        let qualified_name = typing::resolve_assignment(value, checker.semantic())
            .or_else(|| {
                value
                    .as_call_expr()
                    .and_then(|call| checker.semantic().resolve_qualified_name(&call.func))
            })
            .or_else(|| checker.semantic().resolve_qualified_name(value));

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
    let (violation, range) = match (qualified_name.segments(), attr.as_str()) {
        (["airflow", "lineage", "hook", "HookLineageCollector"], "create_asset")
            if arguments.find_positional(0).is_some()
                || arguments.args.iter().any(Expr::is_starred_expr) =>
        {
            (
                Airflow3IncompatibleFunctionSignature {
                    function_name: attr.to_string(),
                    change: FunctionSignatureChange::Message(
                        "Pass positional arguments as keyword arguments (e.g., `create_asset(uri=...)`)",
                    ),
                },
                attr.range(),
            )
        }
        (["airflow", "sdk", "Variable"], "get")
            if arguments.find_keyword("default_var").is_some() =>
        {
            let keyword = arguments.find_keyword("default_var").unwrap();
            (
                Airflow3IncompatibleFunctionSignature {
                    function_name: "Variable.get".to_string(),
                    change: FunctionSignatureChange::ArgName {
                        old: "default_var",
                        new: "default",
                    },
                },
                keyword.arg.as_ref().unwrap().range(),
            )
        }
        _ => return,
    };

    let fix = match &violation.change {
        FunctionSignatureChange::ArgName { new, .. } => Some(Fix::safe_edit(
            Edit::range_replacement((*new).to_string(), range),
        )),
        FunctionSignatureChange::Message(_) => None,
    };

    let mut diagnostic = checker.report_diagnostic(violation, range);
    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }
}

fn check_constructor_arguments(
    checker: &Checker,
    qualified_name: &QualifiedName,
    arguments: &Arguments,
    func: &Expr,
) {
    let (violation, range) = match qualified_name.segments() {
        [
            "airflow",
            "providers",
            "standard",
            "operators",
            "python",
            "PythonOperator"
            | "BranchPythonOperator"
            | "ShortCircuitOperator"
            | "PythonVirtualenvOperator"
            | "BranchPythonVirtualenvOperator"
            | "ExternalPythonOperator"
            | "BranchExternalPythonOperator",
        ]
        | [
            "airflow",
            "operators",
            "python",
            "PythonOperator"
            | "BranchPythonOperator"
            | "ShortCircuitOperator"
            | "PythonVirtualenvOperator"
            | "BranchPythonVirtualenvOperator"
            | "ExternalPythonOperator"
            | "BranchExternalPythonOperator",
        ] if arguments.find_keyword("provide_context").is_some() => {
            let keyword = arguments.find_keyword("provide_context").unwrap();
            (
                Airflow3IncompatibleFunctionSignature {
                    function_name: qualified_name.segments().last().unwrap_or(&"").to_string(),
                    change: FunctionSignatureChange::Message(
                        "`provide_context` is deprecated as of 2.0 and removed in 3.0, which is no longer required.",
                    ),
                },
                keyword.range(),
            )
        }
        ["airflow", "Dataset"]
        | ["airflow", "datasets", "Dataset"]
        | ["airflow", "sdk", "Asset"]
            if arguments
                .find_positional(1)
                .is_some_and(|second_arg| is_dict_expression(checker, second_arg)) =>
        {
            (
                Airflow3IncompatibleFunctionSignature {
                    function_name: qualified_name.segments().last().unwrap_or(&"").to_string(),
                    change: FunctionSignatureChange::Message(
                        "Use keyword argument `extra` instead of passing a dict as the second positional argument (e.g., `Asset(name=..., uri=..., extra=...)`)",
                    ),
                },
                func.range(),
            )
        }
        _ => return,
    };

    checker.report_diagnostic(violation, range);
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
