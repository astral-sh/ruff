use std::ops::Range;
use std::sync::LazyLock;

use regex::{CaptureMatches, Regex};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprSubscript, Parameter, ParameterWithDefault};
use ruff_python_semantic::{BindingKind, Modules, ScopeKind, SemanticModel};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextSize};

use crate::Fix;
use crate::checkers::ast::Checker;
use crate::fix::edits::add_parameter;
use crate::rules::fastapi::rules::is_fastapi_route_decorator;
use crate::{FixAvailability, Violation};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.10.0")]
pub(crate) struct FastApiUnusedPathParameter {
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
        if !is_positional {
            format!(
                "Parameter `{arg_name}` appears in route path, but not in `{function_name}` signature"
            )
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
    checker: &Checker,
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
    let mut named_args = vec![];

    for parameter_with_default in non_posonly_non_variadic_parameters(function_def) {
        let ParameterWithDefault { parameter, .. } = parameter_with_default;

        if let Some(alias) = parameter_alias(parameter, checker.semantic()) {
            named_args.push(alias);
        } else {
            named_args.push(parameter.name.as_str());
        }

        let Some(dependency) =
            Dependency::from_parameter(parameter_with_default, checker.semantic())
        else {
            continue;
        };

        if let Some(parameter_names) = dependency.parameter_names() {
            named_args.extend_from_slice(parameter_names);
        } else {
            // If we can't determine the dependency, we can't really do anything.
            return;
        }
    }

    // Check if any of the path parameters are not in the function signature.
    for (path_param, range) in path_params {
        // If the path parameter is already in the function or the dependency signature,
        // we don't need to do anything.
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
            .any(|param| param.name() == path_param);

        let mut diagnostic = checker.report_diagnostic(
            FastApiUnusedPathParameter {
                arg_name: path_param.to_string(),
                function_name: function_def.name.to_string(),
                is_positional,
            },
            #[expect(clippy::cast_possible_truncation)]
            diagnostic_range
                .add_start(TextSize::from(range.start as u32 + 1))
                .sub_end(TextSize::from((path.len() - range.end + 1) as u32)),
        );
        if !is_positional && is_identifier(path_param) && path_param != "__debug__" {
            diagnostic.set_fix(Fix::unsafe_edit(add_parameter(
                path_param,
                &function_def.parameters,
                checker.locator().contents(),
            )));
        }
    }
}

/// Returns an iterator over the non-positional-only, non-variadic parameters of a function.
fn non_posonly_non_variadic_parameters(
    function_def: &ast::StmtFunctionDef,
) -> impl Iterator<Item = &ParameterWithDefault> {
    function_def
        .parameters
        .args
        .iter()
        .chain(&function_def.parameters.kwonlyargs)
}

#[derive(Debug, is_macro::Is)]
enum Dependency<'a> {
    /// Not defined in the same file, or otherwise cannot be determined to be a function.
    Unknown,

    /// A function defined in the same file, whose parameter names are as given.
    Function(Vec<&'a str>),

    /// A class defined in the same file, whose constructor parameter names are as given.
    Class(Vec<&'a str>),

    /// There are multiple `Depends()` calls.
    ///
    /// Multiple `Depends` annotations aren't supported by fastapi and the exact behavior is
    /// [unspecified](https://github.com/astral-sh/ruff/pull/15364#discussion_r1912551710).
    Multiple,
}

impl<'a> Dependency<'a> {
    fn parameter_names(&self) -> Option<&[&'a str]> {
        match self {
            Self::Unknown => None,
            Self::Multiple => None,
            Self::Function(parameter_names) => Some(parameter_names.as_slice()),
            Self::Class(parameter_names) => Some(parameter_names.as_slice()),
        }
    }
}

impl<'a> Dependency<'a> {
    /// Return `foo` in the first metadata `Depends(foo)`,
    /// or that of the default value:
    ///
    /// ```python
    /// def _(
    ///     a: Annotated[str, Depends(foo)],
    ///     #                 ^^^^^^^^^^^^
    ///     a: str = Depends(foo),
    ///     #        ^^^^^^^^^^^^
    /// ): ...
    /// ```
    fn from_parameter(
        parameter: &'a ParameterWithDefault,
        semantic: &SemanticModel<'a>,
    ) -> Option<Self> {
        if let Some(dependency) = parameter
            .default()
            .and_then(|default| Self::from_default(default, semantic))
        {
            return Some(dependency);
        }

        let ExprSubscript { value, slice, .. } = parameter.annotation()?.as_subscript_expr()?;

        if !semantic.match_typing_expr(value, "Annotated") {
            return None;
        }

        let Expr::Tuple(tuple) = slice.as_ref() else {
            return None;
        };

        let mut dependencies = tuple.elts.iter().skip(1).filter_map(|metadata_element| {
            let arguments = depends_arguments(metadata_element, semantic)?;

            // Arguments to `Depends` can be empty if the dependency is a class
            // that FastAPI will call to create an instance of the class itself.
            // https://fastapi.tiangolo.com/tutorial/dependencies/classes-as-dependencies/#shortcut
            if arguments.is_empty() {
                Self::from_dependency_name(tuple.elts.first()?.as_name_expr()?, semantic)
            } else {
                Self::from_depends_call(arguments, semantic)
            }
        });

        let dependency = dependencies.next()?;

        if dependencies.next().is_some() {
            Some(Self::Multiple)
        } else {
            Some(dependency)
        }
    }

    fn from_default(expr: &'a Expr, semantic: &SemanticModel<'a>) -> Option<Self> {
        let arguments = depends_arguments(expr, semantic)?;

        Self::from_depends_call(arguments, semantic)
    }

    fn from_depends_call(arguments: &'a Arguments, semantic: &SemanticModel<'a>) -> Option<Self> {
        let Some(Expr::Name(name)) = arguments.find_argument_value("dependency", 0) else {
            return None;
        };

        Self::from_dependency_name(name, semantic)
    }

    fn from_dependency_name(name: &'a ast::ExprName, semantic: &SemanticModel<'a>) -> Option<Self> {
        let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
            return Some(Self::Unknown);
        };

        match binding.kind {
            BindingKind::FunctionDefinition(scope_id) => {
                let scope = &semantic.scopes[scope_id];

                let ScopeKind::Function(function_def) = scope.kind else {
                    return Some(Self::Unknown);
                };

                let parameter_names = non_posonly_non_variadic_parameters(function_def)
                    .map(|param| param.name().as_str())
                    .collect();

                Some(Self::Function(parameter_names))
            }
            BindingKind::ClassDefinition(scope_id) => {
                let scope = &semantic.scopes[scope_id];

                let ScopeKind::Class(class_def) = scope.kind else {
                    return Some(Self::Unknown);
                };

                let parameter_names = if class_def
                    .bases()
                    .iter()
                    .any(|expr| is_pydantic_base_model(expr, semantic))
                {
                    class_def
                        .body
                        .iter()
                        .filter_map(|stmt| {
                            stmt.as_ann_assign_stmt()
                                .and_then(|ann_assign| ann_assign.target.as_name_expr())
                                .map(|name| name.id.as_str())
                        })
                        .collect()
                } else if let Some(init_def) = class_def
                    .body
                    .iter()
                    .filter_map(|stmt| stmt.as_function_def_stmt())
                    .find(|func_def| func_def.name.as_str() == "__init__")
                {
                    // Skip `self` parameter
                    non_posonly_non_variadic_parameters(init_def)
                        .skip(1)
                        .map(|param| param.name().as_str())
                        .collect()
                } else {
                    return None;
                };

                Some(Self::Class(parameter_names))
            }
            _ => Some(Self::Unknown),
        }
    }
}

fn depends_arguments<'a>(expr: &'a Expr, semantic: &SemanticModel) -> Option<&'a Arguments> {
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return None;
    };

    if !is_fastapi_depends(func.as_ref(), semantic) {
        return None;
    }

    Some(arguments)
}

fn is_fastapi_depends(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["fastapi", "Depends"]))
}

fn is_pydantic_base_model(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["pydantic", "BaseModel"])
        })
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
///
/// FastAPI only recognizes path parameters when there are no leading or trailing spaces around
/// the parameter name. For example, `/{x}` is a valid parameter, but `/{ x }` is treated literally.
#[derive(Debug)]
struct PathParamIterator<'a> {
    inner: CaptureMatches<'a, 'a>,
}

impl<'a> PathParamIterator<'a> {
    fn new(input: &'a str) -> Self {
        /// Matches the Starlette pattern for path parameters with optional converters from
        /// <https://github.com/Kludex/starlette/blob/e18637c68e36d112b1983bc0c8b663681e6a4c50/starlette/routing.py#L121>
        static FASTAPI_PATH_PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)(?::[a-zA-Z_][a-zA-Z0-9_]*)?\}").unwrap()
        });

        Self {
            inner: FASTAPI_PATH_PARAM_REGEX.captures_iter(input),
        }
    }
}

impl<'a> Iterator for PathParamIterator<'a> {
    type Item = (&'a str, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            // Extract the first capture group (the path parameter), but return the range of the
            // whole match (everything in braces and including the braces themselves).
            .and_then(|capture| Some((capture.get(1)?.as_str(), capture.get(0)?.range())))
    }
}
