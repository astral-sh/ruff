use crate::checkers::ast::Checker;
use ruff_python_ast::{Decorator, ExprCall, StmtFunctionDef};
use ruff_python_semantic::analyze::typing::resolve_assignment;

pub(crate) fn is_fastapi_route(checker: &Checker, function_def: &StmtFunctionDef) -> bool {
    return function_def
        .decorator_list
        .iter()
        .any(|decorator| is_fastapi_route_decorator(checker, decorator).is_some());
}

pub(crate) fn is_fastapi_route_decorator<'a>(
    checker: &'a Checker,
    decorator: &'a Decorator,
) -> Option<&'a ExprCall> {
    let call = decorator.expression.as_call_expr()?;
    let decorator_method = call.func.as_attribute_expr()?;
    let method_name = &decorator_method.attr;

    let route_methods = [
        "get", "post", "put", "delete", "patch", "options", "head", "trace",
    ];
    if !route_methods.contains(&method_name.as_str()) {
        return None;
    }
    let ra = resolve_assignment(&decorator_method.value, checker.semantic());
    if !ra.is_some_and(|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["fastapi", "FastAPI" | "APIRouter"]
        )
    }) {
        return None;
    }
    Some(call)
}
