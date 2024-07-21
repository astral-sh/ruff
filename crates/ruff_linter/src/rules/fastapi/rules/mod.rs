pub(crate) use fastapi_non_annotated_dependency::*;
pub(crate) use fastapi_redundant_response_model::*;

mod fastapi_non_annotated_dependency;
mod fastapi_redundant_response_model;

use ruff_python_ast::{Decorator, ExprCall, StmtFunctionDef};
use ruff_python_semantic::analyze::typing::resolve_assignment;
use ruff_python_semantic::SemanticModel;

/// Returns `true` if the function is a FastAPI route.
pub(crate) fn is_fastapi_route(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    return function_def
        .decorator_list
        .iter()
        .any(|decorator| is_fastapi_route_decorator(decorator, semantic).is_some());
}

/// Returns `true` if the decorator is indicative of a FastAPI route.
pub(crate) fn is_fastapi_route_decorator<'a>(
    decorator: &'a Decorator,
    semantic: &'a SemanticModel,
) -> Option<&'a ExprCall> {
    let call = decorator.expression.as_call_expr()?;
    let decorator_method = call.func.as_attribute_expr()?;
    let method_name = &decorator_method.attr;

    if !matches!(
        method_name.as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace"
    ) {
        return None;
    }

    let qualified_name = resolve_assignment(&decorator_method.value, semantic)?;
    if matches!(
        qualified_name.segments(),
        ["fastapi", "FastAPI" | "APIRouter"]
    ) {
        Some(call)
    } else {
        None
    }
}
