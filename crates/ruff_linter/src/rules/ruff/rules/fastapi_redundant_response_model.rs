use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::typing::resolve_assignment;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for FastApi routes that uses the optional `response_model` parameter with the same type as the return type.
///
/// ## Why is this bad?
/// FastApi routes automatically infer the response model from the return type, so specifying it explicitly is redundant.
/// `Response_model` is used to override the default response model, for example,
/// when the function returns an unstructured response and FastAPI should use a specific model for it.
///
/// ## Example
/// ```python
/// from fastapi import FastAPI
/// from pydantic import BaseModel
///
/// app = FastAPI()
/// class Item(BaseModel):
///     name: str
///
///
/// @app.post("/items/", response_model=Item)
/// async def create_item(item: Item) -> Item:
///     return item
/// ```
///
/// Use instead:
/// ```python
/// from fastapi import FastAPI
/// from pydantic import BaseModel
///
/// app = FastAPI()
/// class Item(BaseModel):
///     name: str
///
///
/// @app.post("/items/")
/// async def create_item(item: Item) -> Item:
///     return item
/// ```
#[violation]
pub struct FastApiRedundantResponseModel;

impl Violation for FastApiRedundantResponseModel {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("FastAPI route with redundant response_model parameter")
    }
}

/// RUF102
pub(crate) fn fastapi_redundant_response_model(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    // Check if the function has a fast api app.post decorator
    for decorator in &function_def.decorator_list {
        let Some(call) = decorator.expression.as_call_expr() else {
            continue;
        };
        let Some(decorator_method) = call.func.as_attribute_expr() else {
            continue;
        };
        let method_name = &decorator_method.attr;

        let route_methods = [
            "get", "post", "put", "delete", "patch", "options", "head", "trace",
        ];
        if !route_methods.contains(&method_name.as_str()) {
            continue;
        }
        let ra = resolve_assignment(&*decorator_method.value, checker.semantic());
        if !ra.is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["fastapi", "FastAPI"] | ["fastapi", "APIRouter"]
            )
        }) {
            continue;
        }
        let Some(response_model_arg) = call.arguments.find_keyword("response_model") else {
            continue;
        };
        checker.diagnostics.push(Diagnostic::new(
            FastApiRedundantResponseModel,
            response_model_arg.range(),
        ));
    }
}
