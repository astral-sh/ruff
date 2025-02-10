pub(crate) use fastapi_non_annotated_dependency::*;
pub(crate) use fastapi_redundant_response_model::*;
pub(crate) use fastapi_unused_path_parameter::*;

mod fastapi_non_annotated_dependency;
mod fastapi_redundant_response_model;
mod fastapi_unused_path_parameter;

use ruff_python_ast as ast;
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;

/// Returns `true` if the function is a FastAPI route.
pub(crate) fn is_fastapi_route(
    function_def: &ast::StmtFunctionDef,
    semantic: &SemanticModel,
) -> bool {
    function_def
        .decorator_list
        .iter()
        .any(|decorator| is_fastapi_route_decorator(decorator, semantic).is_some())
}

/// Returns `true` if the decorator is indicative of a FastAPI route.
pub(crate) fn is_fastapi_route_decorator<'a>(
    decorator: &'a ast::Decorator,
    semantic: &'a SemanticModel,
) -> Option<&'a ast::ExprCall> {
    let call = decorator.expression.as_call_expr()?;
    is_fastapi_route_call(call, semantic).then_some(call)
}

pub(crate) fn is_fastapi_route_call(call_expr: &ast::ExprCall, semantic: &SemanticModel) -> bool {
    let ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = &*call_expr.func else {
        return false;
    };

    if !matches!(
        attr.as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace"
    ) {
        return false;
    }
    let Some(name) = value.as_name_expr() else {
        return false;
    };
    let Some(binding_id) = semantic.resolve_name(name) else {
        return false;
    };
    typing::is_fastapi_route(semantic.binding(binding_id), semantic)
}
