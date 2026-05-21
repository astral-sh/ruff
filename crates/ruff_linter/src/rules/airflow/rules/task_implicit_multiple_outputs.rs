use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{ReturnStatementVisitor, map_callable};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Decorator, Expr, ExprAttribute, Stmt, StmtFunctionDef};
use ruff_python_semantic::analyze;
use ruff_python_semantic::{BindingKind, Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `@task`-decorated functions whose `multiple_outputs` behavior is
/// determined by Airflow's runtime inference rather than being set explicitly.
///
/// ## Why is this bad?
/// At runtime, Airflow infers `multiple_outputs` from the return type
/// annotation: if it resolves to a subclass of `collections.abc.Mapping`, the
/// return value is split into one `XCom` per key; otherwise it is stored as a
/// single `XCom`. This couples typing to `XCom` layout in a non-obvious way —
/// renaming, removing, or refining the return annotation silently changes the
/// Dag's `XCom` behavior.
///
/// Passing `multiple_outputs=` explicitly makes the author's intent clear,
/// insulates the Dag from future changes to inference, and increases
/// awareness of the parameter.
///
/// ## Example
/// ```python
/// from airflow.sdk import task
///
///
/// @task
/// def extract() -> dict:
///     return {"x": 1, "y": 2}
/// ```
///
/// Use instead:
/// ```python
/// from airflow.sdk import task
///
///
/// @task(multiple_outputs=True)
/// def extract() -> dict:
///     return {"x": 1, "y": 2}
/// ```
///
/// ## Fix safety
/// The fix is always marked unsafe: the inserted value mirrors Airflow's
/// current inference (`True` when the return annotation is a `Mapping`
/// subclass, `False` otherwise), but the author may have intended a different
/// `XCom` layout, and a function with multiple return paths may not always
/// return a dict.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.14")]
pub(crate) struct AirflowTaskImplicitMultipleOutputs {
    annotation_is_mapping: bool,
}

impl Violation for AirflowTaskImplicitMultipleOutputs {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`@task`-decorated function relies on `multiple_outputs` inference".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        if self.annotation_is_mapping {
            Some("Add `multiple_outputs=True`".to_string())
        } else {
            Some("Add `multiple_outputs=False`".to_string())
        }
    }
}

/// AIR202
pub(crate) fn task_implicit_multiple_outputs(checker: &Checker, function_def: &StmtFunctionDef) {
    let semantic = checker.semantic();
    if !semantic.seen_module(Modules::AIRFLOW) {
        return;
    }

    let Some(decorator) = matching_task_decorator(function_def, semantic) else {
        return;
    };

    // If `multiple_outputs` is already specified, nothing to do.
    if decorator_has_multiple_outputs(decorator) {
        return;
    }

    let annotation_is_mapping = function_def
        .returns
        .as_deref()
        .is_some_and(|annotation| annotation_resolves_to_mapping(annotation, semantic));

    if !annotation_is_mapping && !body_has_dict_return(function_def, semantic) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        AirflowTaskImplicitMultipleOutputs {
            annotation_is_mapping,
        },
        decorator.range(),
    );
    diagnostic.set_fix(build_fix(decorator, annotation_is_mapping, checker));
}

/// Return the matched `@task` (or supported `@task.<variant>`) decorator on
/// `function_def`, or `None` if there is none.
fn matching_task_decorator<'a>(
    function_def: &'a StmtFunctionDef,
    semantic: &SemanticModel,
) -> Option<&'a Decorator> {
    function_def
        .decorator_list
        .iter()
        .find(|decorator| is_supported_task_decorator(decorator, semantic))
}

fn is_supported_task_decorator(decorator: &Decorator, semantic: &SemanticModel) -> bool {
    let expr = map_callable(&decorator.expression);

    // `@task` or `@task()`.
    if semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qn| matches!(qn.segments(), ["airflow", "decorators" | "sdk", "task"]))
    {
        return true;
    }

    // `@task.<variant>` or `@task.<variant>()`. `task.sensor` is intentionally
    // excluded because the sensor decorator hardcodes `multiple_outputs=False`.
    if let Expr::Attribute(ExprAttribute { value, attr, .. }) = expr {
        if !matches!(
            attr.as_str(),
            "python"
                | "virtualenv"
                | "external_python"
                | "branch"
                | "branch_virtualenv"
                | "branch_external_python"
                | "short_circuit"
                | "docker"
                | "kubernetes"
                | "pyspark"
        ) {
            return false;
        }
        return semantic
            .resolve_qualified_name(value)
            .is_some_and(|qn| matches!(qn.segments(), ["airflow", "decorators" | "sdk", "task"]));
    }

    false
}

/// Returns `true` if the decorator already specifies `multiple_outputs=...`.
fn decorator_has_multiple_outputs(decorator: &Decorator) -> bool {
    let Expr::Call(call) = &decorator.expression else {
        return false;
    };
    call.arguments.find_keyword("multiple_outputs").is_some()
}

/// Returns `true` if `annotation` (the return type expression) resolves to a
/// subclass of `collections.abc.Mapping`, mirroring Airflow's
/// `_infer_multiple_outputs` runtime check.
fn annotation_resolves_to_mapping(annotation: &Expr, semantic: &SemanticModel) -> bool {
    // Unwrap subscripts: `dict[str, int]` → `dict`.
    let head = match annotation {
        Expr::Subscript(ast::ExprSubscript { value, .. }) => value.as_ref(),
        other => other,
    };

    if semantic.match_builtin_expr(head, "dict") {
        return true;
    }

    let typing_targets = [
        "Dict",
        "Mapping",
        "MutableMapping",
        "OrderedDict",
        "DefaultDict",
        "Counter",
        "ChainMap",
        "TypedDict",
    ];
    if typing_targets
        .iter()
        .any(|target| semantic.match_typing_expr(head, target))
    {
        return true;
    }

    if let Some(qn) = semantic.resolve_qualified_name(head)
        && matches!(
            qn.segments(),
            ["collections", "abc", "Mapping" | "MutableMapping"]
                | [
                    "collections",
                    "OrderedDict" | "defaultdict" | "Counter" | "ChainMap"
                ]
        )
    {
        return true;
    }

    annotation_is_typed_dict_subclass(head, semantic)
}

/// Returns `true` if `annotation` is a `Name` referencing a class defined in
/// this module whose base-class chain includes `typing.TypedDict`.
fn annotation_is_typed_dict_subclass(annotation: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Name(name) = annotation else {
        return false;
    };
    let Some(binding_id) = semantic
        .resolve_name(name)
        .or_else(|| semantic.lookup_symbol(&name.id))
    else {
        return false;
    };
    let binding = semantic.binding(binding_id);
    if !matches!(binding.kind, BindingKind::ClassDefinition(_)) {
        return false;
    }
    let Some(Stmt::ClassDef(class_def)) = binding.statement(semantic) else {
        return false;
    };
    analyze::class::any_qualified_base_class(class_def, semantic, |qualified_name| {
        semantic.match_typing_qualified_name(&qualified_name, "TypedDict")
    })
}

/// Returns `true` if any return statement in the function body returns an
/// inline dict literal, a dict comprehension, or a `dict(...)` call.
fn body_has_dict_return(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    let mut visitor = ReturnStatementVisitor::default();
    visitor.visit_body(&function_def.body);
    visitor.returns.iter().any(|ret| {
        let Some(value) = ret.value.as_deref() else {
            return false;
        };
        match value {
            Expr::Dict(_) | Expr::DictComp(_) => true,
            Expr::Call(call) => semantic.match_builtin_expr(&call.func, "dict"),
            _ => false,
        }
    })
}

fn build_fix(decorator: &Decorator, annotation_is_mapping: bool, checker: &Checker) -> Fix {
    let kwarg = if annotation_is_mapping {
        "multiple_outputs=True"
    } else {
        "multiple_outputs=False"
    };

    let edit = match &decorator.expression {
        Expr::Call(call) => add_argument(kwarg, &call.arguments, checker.tokens()),
        // Bare `@task` / `@task.branch` — convert to call form.
        other => Edit::insertion(format!("({kwarg})"), other.range().end()),
    };

    Fix::unsafe_edit(edit)
}
