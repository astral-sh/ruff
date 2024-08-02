use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::fastapi::rules::is_fastapi_route;
use crate::rules::fastapi::rules::is_fastapi_route_decorator;
use regex::Regex;
use ruff_python_stdlib::identifiers::is_identifier;

/// ## What it does
/// Identifies FastAPI routes that declare path parameters in the route path but not in the function signature.
///
/// ## Why is this bad?
/// Path parameters are used to extract values from the URL path.
/// If a path parameter is declared in the route path but not in the function signature, it will not be accessible in the function body, which is likely a mistake.
///
/// ## Known problems
/// If the path parameter is not a valid Python identifier, FastAPI will normalize it to a valid identifier.
/// This lint simply ignores path parameters that are not valid identifiers, as that normalization behavior is undocumented.
///
/// ## Example
///
/// ```python
/// from fastapi import FastAPI
///
/// app = FastAPI()
///
/// @app.get("/things/{thing_id}")
/// async def read_thing(query: str):
///     ...
/// ```
///
/// Use instead:
///
/// ```python
/// from fastapi import FastAPI
///
/// app = FastAPI()
///
/// @app.get("/things/{thing_id}")
/// async def read_thing(thing_id: int, query: str):
///     ...
/// ```

#[violation]
pub struct FastApiUnusedPathParameter {
    arg_name: String,
    function_name: String,
}

impl Violation for FastApiUnusedPathParameter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            arg_name,
            function_name,
        } = self;
        format!(
            "Path parameter `{arg_name}` in route path but not in function signature `{function_name}`"
        )
    }
}

fn extract_path_params_from_route(input: &str) -> Vec<String> {
    // We ignore text after a colon, since those are path convertors
    // See also: https://fastapi.tiangolo.com/tutorial/path-params/?h=path#path-convertor
    let re = Regex::new(r"\{([^:}]+)").unwrap();

    // Collect all matches and return them as a vector of strings
    re.captures_iter(input)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
        .collect()
}

pub(crate) fn fastapi_unused_path_parameter(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }
    if !is_fastapi_route(function_def, checker.semantic()) {
        return;
    }

    // Get the route path from the decorator
    let route_decorator = function_def
        .decorator_list
        .iter()
        .find_map(|decorator| is_fastapi_route_decorator(decorator, checker.semantic()));

    let Some(route_decorator) = route_decorator else {
        return;
    };

    let path_arg = route_decorator.arguments.args.first();
    let Some(path_arg) = path_arg else {
        return;
    };
    // Lets path_arg go out of scope so we can reuse checker later
    let diagnostic_range = path_arg.range();
    // We can't really handle anything other than string literals
    let path = match path_arg.as_string_literal_expr() {
        Some(path_arg) => path_arg.value.to_string(),
        None => return,
    };

    let path_params = extract_path_params_from_route(&path);

    // Now we extract the arguments from the function signature
    let args = function_def
        .parameters
        .args
        .iter()
        .chain(function_def.parameters.kwonlyargs.iter())
        .map(|arg| arg.parameter.name.to_string())
        .collect::<Vec<_>>();

    // Check if any of the path parameters are not in the function signature
    for path_param in path_params
        .into_iter()
        .filter(|path_param| is_identifier(path_param))
    {
        if !args.contains(&path_param) {
            let diagnostic = Diagnostic::new(
                FastApiUnusedPathParameter {
                    arg_name: path_param.clone(),
                    function_name: function_def.name.to_string(),
                },
                diagnostic_range,
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}
