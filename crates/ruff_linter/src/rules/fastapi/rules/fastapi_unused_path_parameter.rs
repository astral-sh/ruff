use std::ops::Range;

use ruff_diagnostics::{Applicability, Fix};
use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;
use ruff_text_size::TextSize;

use crate::checkers::ast::Checker;
use crate::fix::edits::add_parameter;
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
///
/// @app.get("/things/{thing_id}")
/// async def read_thing(thing_id: int, query: str):
///     ...
/// ```

#[violation]
pub struct FastApiUnusedPathParameter {
    arg_name: String,
    function_name: String,
    arg_name_already_used: bool,
}

impl Violation for FastApiUnusedPathParameter {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            arg_name,
            function_name,
            arg_name_already_used: arg_name_is_pos_only_arg,
        } = self;
        if *arg_name_is_pos_only_arg {
            format!(
                "Path parameter `{arg_name}` in route path, but appears as a positional-only argument in function `{function_name}`. Consider making it a regular argument."
            )
        } else {
            format!(
                "Path parameter `{arg_name}` in route path but not in function `{function_name}`."
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Self { arg_name, .. } = self;
        if self.arg_name_already_used {
            None
        } else {
            Some(format!("Add `{arg_name}` to function signature"))
        }
    }
}

/// Returns a vector of path parameters and their ranges in the route path.
/// The string is just the name of the path parameter.
/// The range includes the curly braces.    
fn extract_path_params_from_route(input: &str) -> Vec<(&str, Range<usize>)> {
    // We ignore text after a colon, since those are path convertors
    // See also: https://fastapi.tiangolo.com/tutorial/path-params/?h=path#path-convertor
    let re = Regex::new(r"\{([^:}]+).*?\}").unwrap();

    // Collect all matches and return them as a vector of strings
    re.captures_iter(input)
        .filter_map(|cap| {
            if let (Some(name_match), Some(full_match)) = (cap.get(1), cap.get(0)) {
                Some((name_match.as_str().trim(), full_match.range()))
            } else {
                None
            }
        })
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
    let named_args = function_def
        .parameters
        .args
        .iter()
        .chain(function_def.parameters.kwonlyargs.iter())
        .map(|arg| arg.parameter.name.as_str())
        .collect::<Vec<_>>();

    // Check if any of the path parameters are not in the function signature
    for (path_param, range) in path_params
        .into_iter()
        .filter(|(path_param, _)| is_identifier(path_param))
    {
        if !named_args.contains(&path_param) {
            let violation = FastApiUnusedPathParameter {
                arg_name: path_param.to_string(),
                function_name: function_def.name.to_string(),
                // If the path parameter shows up in the positional-only arguments,
                // the path parameter injection also won't work, but we can't fix that (yet)
                // as that would require making that parameter non positional.
                arg_name_already_used: function_def
                    .parameters
                    .posonlyargs
                    .iter()
                    .map(|arg| arg.parameter.name.as_str())
                    .collect::<Vec<_>>()
                    .contains(&path_param),
            };
            let fixable = violation.fix_title().is_some();
            #[allow(clippy::cast_possible_truncation)]
            let mut diagnostic = Diagnostic::new(
                violation,
                diagnostic_range
                    .add_start(TextSize::from(range.start as u32 + 1))
                    .sub_end(TextSize::from((path.len() - range.end + 1) as u32)),
            );
            if fixable {
                diagnostic.set_fix(Fix::applicable_edit(
                    add_parameter(
                        path_param,
                        &function_def.parameters,
                        checker.locator().contents(),
                    ),
                    Applicability::Safe,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
