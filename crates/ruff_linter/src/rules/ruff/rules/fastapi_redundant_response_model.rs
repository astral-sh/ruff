use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{Decorator, Expr, ExprCall, Keyword, StmtFunctionDef};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::rules::ruff::fastapi::{is_fastapi_route, is_fastapi_route_decorator};

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
///
/// ```python
/// from fastapi import FastAPI
/// from pydantic import BaseModel
///
/// app = FastAPI()
///
///
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
///
/// ```python
/// from fastapi import FastAPI
/// from pydantic import BaseModel
///
/// app = FastAPI()
///
///
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
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }
    if !is_fastapi_route(checker, function_def) {
        return;
    }
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
    let call = is_fastapi_route_decorator(checker, decorator)?;
    let response_model_arg = call.arguments.find_keyword("response_model")?;
    let return_value = function_def.returns.as_ref()?;
    if !is_identical_types(&response_model_arg.value, return_value, checker) {
        return None;
    }
    Some((call, response_model_arg))
}

fn is_identical_types(response_model_arg: &Expr, return_value: &Expr, checker: &Checker) -> bool {
    if let (Some(response_mode_name_expr), Some(return_value_name_expr)) = (
        response_model_arg.as_name_expr(),
        return_value.as_name_expr(),
    ) {
        return checker.semantic().resolve_name(response_mode_name_expr)
            == checker.semantic().resolve_name(return_value_name_expr);
    }
    if let (Some(response_mode_subscript), Some(return_value_subscript)) = (
        response_model_arg.as_subscript_expr(),
        return_value.as_subscript_expr(),
    ) {
        return is_identical_types(
            &response_mode_subscript.value,
            &return_value_subscript.value,
            checker,
        ) && is_identical_types(
            &response_mode_subscript.slice,
            &return_value_subscript.slice,
            checker,
        );
    }
    if let (Some(response_mode_tuple), Some(return_value_tuple)) = (
        response_model_arg.as_tuple_expr(),
        return_value.as_tuple_expr(),
    ) {
        return if response_mode_tuple.elts.len() == return_value_tuple.elts.len() {
            response_mode_tuple
                .elts
                .iter()
                .zip(return_value_tuple.elts.iter())
                .all(|(x, y)| is_identical_types(x, y, checker))
        } else {
            false
        };
    }
    false
}
