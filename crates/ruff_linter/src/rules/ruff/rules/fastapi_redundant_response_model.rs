use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{Decorator, ExprCall, Keyword, StmtFunctionDef};
use ruff_python_semantic::analyze::typing::resolve_assignment;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for FastApi routes that uses the optional `response_model` parameter with the same type as the return type.
///
/// ## Why is this bad?
/// FastApi routes automatically infer the response model from the return type, so specifying it explicitly is redundant.
/// `Response_model` is used to override the default response model, for example,
/// when the function returns a non-serializable type and fastapi should serialize it to a different type.
/// For more information, see the [FastAPI documentation](https://fastapi.tiangolo.com/tutorial/response-model/).
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

impl AlwaysFixableViolation for FastApiRedundantResponseModel {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("FastAPI route with redundant response_model argument")
    }

    fn fix_title(&self) -> String {
        "Remove redundant response_model argument".to_string()
    }
}

/// RUF102
pub(crate) fn fastapi_redundant_response_model(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    // Check if the function has a fast api app.post decorator
    for decorator in &function_def.decorator_list {
        let Some((call, response_model_arg)) = check_decorator(checker, function_def, decorator)
        else {
            continue;
        };
        let mut diagnostic =
            Diagnostic::new(FastApiRedundantResponseModel, response_model_arg.range());
        diagnostic.try_set_fix(|| {
            remove_argument(
                response_model_arg,
                &call.arguments,
                Parentheses::Preserve,
                checker.locator().contents(),
            )
            .map(Fix::unsafe_edit)
        });
        checker.diagnostics.push(diagnostic);
    }
}

fn check_decorator<'a>(
    checker: &'a Checker,
    function_def: &StmtFunctionDef,
    decorator: &'a Decorator,
) -> Option<(&'a ExprCall, &'a Keyword)> {
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
    let response_model_arg = call.arguments.find_keyword("response_model")?;
    let return_value = function_def.returns.clone()?;
    let response_mode_name_expr = response_model_arg.value.as_name_expr()?;
    let return_value_name_expr = return_value.as_name_expr()?;
    let is_response_model_redundant = checker.semantic().resolve_name(response_mode_name_expr)
        == checker.semantic().resolve_name(return_value_name_expr);
    if !is_response_model_redundant {
        return None;
    }
    Some((call, response_model_arg))
}
