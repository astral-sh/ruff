use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Decorator, Expr, ExprCall, Keyword, StmtFunctionDef};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::rules::fastapi::rules::is_fastapi_route_decorator;

/// ## What it does
/// Checks for FastAPI routes that use the optional `response_model` parameter
/// with the same type as the return type.
///
/// ## Why is this bad?
/// FastAPI routes automatically infer the response model type from the return
/// type, so specifying it explicitly is redundant.
///
/// The `response_model` parameter is used to override the default response
/// model type. For example, `response_model` can be used to specify that
/// a non-serializable response type should instead be serialized via an
/// alternative type.
///
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

#[derive(ViolationMetadata)]
pub(crate) struct FastApiRedundantResponseModel;

impl AlwaysFixableViolation for FastApiRedundantResponseModel {
    #[derive_message_formats]
    fn message(&self) -> String {
        "FastAPI route with redundant `response_model` argument".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove argument".to_string()
    }
}

/// FAST001
pub(crate) fn fastapi_redundant_response_model(checker: &Checker, function_def: &StmtFunctionDef) {
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }
    for decorator in &function_def.decorator_list {
        let Some((call, response_model_arg)) =
            check_decorator(function_def, decorator, checker.semantic())
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
        checker.report_diagnostic(diagnostic);
    }
}

fn check_decorator<'a>(
    function_def: &StmtFunctionDef,
    decorator: &'a Decorator,
    semantic: &'a SemanticModel,
) -> Option<(&'a ExprCall, &'a Keyword)> {
    let call = is_fastapi_route_decorator(decorator, semantic)?;
    let response_model_arg = call.arguments.find_keyword("response_model")?;
    let return_value = function_def.returns.as_ref()?;
    if is_identical_types(&response_model_arg.value, return_value, semantic) {
        Some((call, response_model_arg))
    } else {
        None
    }
}

fn is_identical_types(
    response_model_arg: &Expr,
    return_value: &Expr,
    semantic: &SemanticModel,
) -> bool {
    if let (Expr::Name(response_mode_name_expr), Expr::Name(return_value_name_expr)) =
        (response_model_arg, return_value)
    {
        return semantic.resolve_name(response_mode_name_expr)
            == semantic.resolve_name(return_value_name_expr);
    }
    if let (Expr::Subscript(response_mode_subscript), Expr::Subscript(return_value_subscript)) =
        (response_model_arg, return_value)
    {
        return is_identical_types(
            &response_mode_subscript.value,
            &return_value_subscript.value,
            semantic,
        ) && is_identical_types(
            &response_mode_subscript.slice,
            &return_value_subscript.slice,
            semantic,
        );
    }
    if let (Expr::Tuple(response_mode_tuple), Expr::Tuple(return_value_tuple)) =
        (response_model_arg, return_value)
    {
        return response_mode_tuple.len() == return_value_tuple.len()
            && response_mode_tuple
                .iter()
                .zip(return_value_tuple)
                .all(|(x, y)| is_identical_types(x, y, semantic));
    }
    false
}
