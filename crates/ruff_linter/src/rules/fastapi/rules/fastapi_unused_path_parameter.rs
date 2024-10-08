use std::iter::Peekable;
use std::ops::Range;
use std::str::CharIndices;

use ruff_diagnostics::Fix;
use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Parameter, ParameterWithDefault};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::edits::add_parameter;
use crate::rules::fastapi::rules::is_fastapi_route_decorator;

/// ## What it does
/// Identifies FastAPI routes that declare path parameters in the route path
/// that are not included in the function signature.
///
/// ## Why is this bad?
/// Path parameters are used to extract values from the URL path.
///
/// If a path parameter is declared in the route path but not in the function
/// signature, it will not be accessible in the function body, which is likely
/// a mistake.
///
/// If a path parameter is declared in the route path, but as a positional-only
/// argument in the function signature, it will also not be accessible in the
/// function body, as FastAPI will not inject the parameter.
///
/// ## Known problems
/// If the path parameter is _not_ a valid Python identifier (e.g., `user-id`, as
/// opposed to `user_id`), FastAPI will normalize it. However, this rule simply
/// ignores such path parameters, as FastAPI's normalization behavior is undocumented.
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
/// async def read_thing(query: str): ...
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
/// async def read_thing(thing_id: int, query: str): ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as modifying a function signature can
/// change the behavior of the code.
#[violation]
pub struct FastApiUnusedPathParameter {
    arg_name: String,
    function_name: String,
    is_positional: bool,
}

impl Violation for FastApiUnusedPathParameter {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            arg_name,
            function_name,
            is_positional,
        } = self;
        #[allow(clippy::if_not_else)]
        if !is_positional {
            format!("Parameter `{arg_name}` appears in route path, but not in `{function_name}` signature")
        } else {
            format!(
                "Parameter `{arg_name}` appears in route path, but only as a positional-only argument in `{function_name}` signature"
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Self {
            arg_name,
            is_positional,
            ..
        } = self;
        if *is_positional {
            None
        } else {
            Some(format!("Add `{arg_name}` to function signature"))
        }
    }
}

/// FAST003
pub(crate) fn fastapi_unused_path_parameter(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }

    // Get the route path from the decorator.
    let route_decorator = function_def
        .decorator_list
        .iter()
        .find_map(|decorator| is_fastapi_route_decorator(decorator, checker.semantic()));

    let Some(route_decorator) = route_decorator else {
        return;
    };

    let Some(path_arg) = route_decorator.arguments.args.first() else {
        return;
    };
    let diagnostic_range = path_arg.range();

    // We can't really handle anything other than string literals.
    let path = match path_arg.as_string_literal_expr() {
        Some(path_arg) => &path_arg.value,
        None => return,
    };

    // Extract the path parameters from the route path.
    let path_params = PathParamIterator::new(path.to_str());

    // Extract the arguments from the function signature
    let named_args: Vec<_> = function_def
        .parameters
        .args
        .iter()
        .chain(function_def.parameters.kwonlyargs.iter())
        .map(|ParameterWithDefault { parameter, .. }| {
            parameter_alias(parameter, checker.semantic())
                .unwrap_or_else(|| parameter.name.as_str())
        })
        .collect();

    // Check if any of the path parameters are not in the function signature.
    let mut diagnostics = vec![];
    for (path_param, range) in path_params {
        // Ignore invalid identifiers (e.g., `user-id`, as opposed to `user_id`)
        if !is_identifier(path_param) {
            continue;
        }

        // If the path parameter is already in the function signature, we don't need to do anything.
        if named_args.contains(&path_param) {
            continue;
        }

        // Determine whether the path parameter is used as a positional-only argument. In this case,
        // the path parameter injection won't work, but we also can't fix it (yet), since we'd need
        // to make the parameter non-positional-only.
        let is_positional = function_def
            .parameters
            .posonlyargs
            .iter()
            .any(|arg| arg.parameter.name.as_str() == path_param);

        let mut diagnostic = Diagnostic::new(
            FastApiUnusedPathParameter {
                arg_name: path_param.to_string(),
                function_name: function_def.name.to_string(),
                is_positional,
            },
            #[allow(clippy::cast_possible_truncation)]
            diagnostic_range
                .add_start(TextSize::from(range.start as u32 + 1))
                .sub_end(TextSize::from((path.len() - range.end + 1) as u32)),
        );
        if !is_positional {
            diagnostic.set_fix(Fix::unsafe_edit(add_parameter(
                path_param,
                &function_def.parameters,
                checker.locator().contents(),
            )));
        }
        diagnostics.push(diagnostic);
    }

    checker.diagnostics.extend(diagnostics);
}

/// Extract the expected in-route name for a given parameter, if it has an alias.
/// For example, given `document_id: Annotated[str, Path(alias="documentId")]`, returns `"documentId"`.
fn parameter_alias<'a>(parameter: &'a Parameter, semantic: &SemanticModel) -> Option<&'a str> {
    let Some(annotation) = &parameter.annotation else {
        return None;
    };

    let Expr::Subscript(subscript) = annotation.as_ref() else {
        return None;
    };

    let Expr::Tuple(tuple) = subscript.slice.as_ref() else {
        return None;
    };

    let Some(Expr::Call(path)) = tuple.elts.get(1) else {
        return None;
    };

    // Find the `alias` keyword argument.
    let alias = path
        .arguments
        .find_keyword("alias")
        .map(|alias| &alias.value)?;

    // Ensure that it's a literal string.
    let Expr::StringLiteral(alias) = alias else {
        return None;
    };

    // Verify that the subscript was a `typing.Annotated`.
    if !semantic.match_typing_expr(&subscript.value, "Annotated") {
        return None;
    }

    // Verify that the call was a `fastapi.Path`.
    if !semantic
        .resolve_qualified_name(&path.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["fastapi", "Path"]))
    {
        return None;
    }

    Some(alias.value.to_str())
}

/// An iterator to extract parameters from FastAPI route paths.
///
/// The iterator yields tuples of the parameter name and the range of the parameter in the input,
/// inclusive of curly braces.
#[derive(Debug)]
struct PathParamIterator<'a> {
    input: &'a str,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> PathParamIterator<'a> {
    fn new(input: &'a str) -> Self {
        PathParamIterator {
            input,
            chars: input.char_indices().peekable(),
        }
    }
}

impl<'a> Iterator for PathParamIterator<'a> {
    type Item = (&'a str, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((start, c)) = self.chars.next() {
            if c == '{' {
                if let Some((end, _)) = self.chars.by_ref().find(|&(_, ch)| ch == '}') {
                    let param_content = &self.input[start + 1..end];
                    // We ignore text after a colon, since those are path convertors
                    // See also: https://fastapi.tiangolo.com/tutorial/path-params/?h=path#path-convertor
                    let param_name_end = param_content.find(':').unwrap_or(param_content.len());
                    let param_name = &param_content[..param_name_end].trim();

                    #[allow(clippy::range_plus_one)]
                    return Some((param_name, start..end + 1));
                }
            }
        }
        None
    }
}
