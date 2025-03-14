use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::Modules;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::fastapi::rules::is_fastapi_route;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Identifies FastAPI routes with deprecated uses of `Depends` or similar.
///
/// ## Why is this bad?
/// The [FastAPI documentation] recommends the use of [`typing.Annotated`][typing-annotated]
/// for defining route dependencies and parameters, rather than using `Depends`,
/// `Query` or similar as a default value for a parameter. Using this approach
/// everywhere helps ensure consistency and clarity in defining dependencies
/// and parameters.
///
/// `Annotated` was added to the `typing` module in Python 3.9; however,
/// the third-party [`typing_extensions`][typing-extensions] package
/// provides a backport that can be used on older versions of Python.
///
/// ## Example
///
/// ```python
/// from fastapi import Depends, FastAPI
///
/// app = FastAPI()
///
///
/// async def common_parameters(q: str | None = None, skip: int = 0, limit: int = 100):
///     return {"q": q, "skip": skip, "limit": limit}
///
///
/// @app.get("/items/")
/// async def read_items(commons: dict = Depends(common_parameters)):
///     return commons
/// ```
///
/// Use instead:
///
/// ```python
/// from typing import Annotated
///
/// from fastapi import Depends, FastAPI
///
/// app = FastAPI()
///
///
/// async def common_parameters(q: str | None = None, skip: int = 0, limit: int = 100):
///     return {"q": q, "skip": skip, "limit": limit}
///
///
/// @app.get("/items/")
/// async def read_items(commons: Annotated[dict, Depends(common_parameters)]):
///     return commons
/// ```
///
/// [FastAPI documentation]: https://fastapi.tiangolo.com/tutorial/query-params-str-validations/?h=annotated#advantages-of-annotated
/// [typing-annotated]: https://docs.python.org/3/library/typing.html#typing.Annotated
/// [typing-extensions]: https://typing-extensions.readthedocs.io/en/stable/
#[derive(ViolationMetadata)]
pub(crate) struct FastApiNonAnnotatedDependency {
    py_version: PythonVersion,
}

impl Violation for FastApiNonAnnotatedDependency {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "FastAPI dependency without `Annotated`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let title = if self.py_version >= PythonVersion::PY39 {
            "Replace with `typing.Annotated`"
        } else {
            "Replace with `typing_extensions.Annotated`"
        };
        Some(title.to_string())
    }
}

/// FAST002
pub(crate) fn fastapi_non_annotated_dependency(
    checker: &Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI)
        || !is_fastapi_route(function_def, checker.semantic())
    {
        return;
    }

    // `create_diagnostic` needs to know if a default argument has been seen to
    // avoid emitting fixes that would remove defaults and cause a syntax error.
    let mut seen_default = false;

    for parameter in function_def
        .parameters
        .args
        .iter()
        .chain(&function_def.parameters.kwonlyargs)
    {
        let (Some(annotation), Some(default)) = (parameter.annotation(), parameter.default())
        else {
            seen_default |= parameter.default.is_some();
            continue;
        };

        if let Some(dependency) = is_fastapi_dependency(checker, default) {
            let dependency_call = DependencyCall::from_expression(default);
            let dependency_parameter = DependencyParameter {
                annotation,
                default,
                kind: dependency,
                name: parameter.name(),
                range: parameter.range,
            };
            seen_default = create_diagnostic(
                checker,
                &dependency_parameter,
                dependency_call,
                seen_default,
            );
        } else {
            seen_default |= parameter.default.is_some();
        }
    }
}

fn is_fastapi_dependency(checker: &Checker, expr: &ast::Expr) -> Option<FastApiDependency> {
    checker
        .semantic()
        .resolve_qualified_name(map_callable(expr))
        .and_then(|qualified_name| match qualified_name.segments() {
            ["fastapi", dependency_name] => match *dependency_name {
                "Query" => Some(FastApiDependency::Query),
                "Path" => Some(FastApiDependency::Path),
                "Body" => Some(FastApiDependency::Body),
                "Cookie" => Some(FastApiDependency::Cookie),
                "Header" => Some(FastApiDependency::Header),
                "File" => Some(FastApiDependency::File),
                "Form" => Some(FastApiDependency::Form),
                "Depends" => Some(FastApiDependency::Depends),
                "Security" => Some(FastApiDependency::Security),
                _ => None,
            },
            _ => None,
        })
}

#[derive(Debug, Copy, Clone)]
enum FastApiDependency {
    Query,
    Path,
    Body,
    Cookie,
    Header,
    File,
    Form,
    Depends,
    Security,
}

struct DependencyParameter<'a> {
    annotation: &'a ast::Expr,
    default: &'a ast::Expr,
    range: TextRange,
    name: &'a str,
    kind: FastApiDependency,
}

struct DependencyCall<'a> {
    default_argument: ast::ArgOrKeyword<'a>,
    keyword_arguments: Vec<&'a ast::Keyword>,
}

impl<'a> DependencyCall<'a> {
    fn from_expression(expr: &'a ast::Expr) -> Option<Self> {
        let call = expr.as_call_expr()?;
        let default_argument = call.arguments.find_argument("default", 0)?;
        let keyword_arguments = call
            .arguments
            .keywords
            .iter()
            .filter(|kwarg| kwarg.arg.as_ref().is_some_and(|name| name != "default"))
            .collect();

        Some(Self {
            default_argument,
            keyword_arguments,
        })
    }
}

/// Create a [`Diagnostic`] for `parameter` and return an updated value of `seen_default`.
///
/// While all of the *input* `parameter` values have default values (see the `needs_update` match in
/// [`fastapi_non_annotated_dependency`]), some of the fixes remove default values. For example,
///
/// ```python
/// def handler(some_path_param: str = Path()): pass
/// ```
///
/// Gets fixed to
///
/// ```python
/// def handler(some_path_param: Annotated[str, Path()]): pass
/// ```
///
/// Causing it to lose its default value. That's fine in this example but causes a syntax error if
/// `some_path_param` comes after another argument with a default. We only compute the information
/// necessary to determine this while generating the fix, thus the need to return an updated
/// `seen_default` here.
fn create_diagnostic(
    checker: &Checker,
    parameter: &DependencyParameter,
    dependency_call: Option<DependencyCall>,
    mut seen_default: bool,
) -> bool {
    let mut diagnostic = Diagnostic::new(
        FastApiNonAnnotatedDependency {
            py_version: checker.target_version(),
        },
        parameter.range,
    );

    let try_generate_fix = || {
        let module = if checker.target_version() >= PythonVersion::PY39 {
            "typing"
        } else {
            "typing_extensions"
        };
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from(module, "Annotated"),
            parameter.range.start(),
            checker.semantic(),
        )?;

        // Each of these classes takes a single, optional default
        // argument, followed by kw-only arguments

        // Refine the match from `is_fastapi_dependency` to exclude Depends
        // and Security, which don't have the same argument structure. The
        // others need to be converted from `q: str = Query("")` to `q:
        // Annotated[str, Query()] = ""` for example, but Depends and
        // Security need to stay like `Annotated[str, Depends(callable)]`
        let is_route_param = !matches!(
            parameter.kind,
            FastApiDependency::Depends | FastApiDependency::Security
        );

        let content = match dependency_call {
            Some(dependency_call) if is_route_param => {
                let kwarg_list = dependency_call
                    .keyword_arguments
                    .iter()
                    .map(|kwarg| checker.locator().slice(kwarg.range()))
                    .collect::<Vec<_>>()
                    .join(", ");

                seen_default = true;
                format!(
                    "{parameter_name}: {binding}[{annotation}, {default_}({kwarg_list})] \
                            = {default_value}",
                    parameter_name = parameter.name,
                    annotation = checker.locator().slice(parameter.annotation.range()),
                    default_ = checker
                        .locator()
                        .slice(map_callable(parameter.default).range()),
                    default_value = checker
                        .locator()
                        .slice(dependency_call.default_argument.value().range()),
                )
            }
            _ => {
                if seen_default {
                    return Ok(None);
                }
                format!(
                    "{parameter_name}: {binding}[{annotation}, {default_}]",
                    parameter_name = parameter.name,
                    annotation = checker.locator().slice(parameter.annotation.range()),
                    default_ = checker.locator().slice(parameter.default.range())
                )
            }
        };
        let parameter_edit = Edit::range_replacement(content, parameter.range);
        Ok(Some(Fix::unsafe_edits(import_edit, [parameter_edit])))
    };

    // make sure we set `seen_default` if we bail out of `try_generate_fix` early. we could
    // `match` on the result directly, but still calling `try_set_optional_fix` avoids
    // duplicating the debug logging here
    let fix: anyhow::Result<Option<Fix>> = try_generate_fix();
    if fix.is_err() {
        seen_default = true;
    }
    diagnostic.try_set_optional_fix(|| fix);

    checker.report_diagnostic(diagnostic);

    seen_default
}
