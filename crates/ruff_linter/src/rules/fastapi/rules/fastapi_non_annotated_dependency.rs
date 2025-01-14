use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::fastapi::rules::is_fastapi_route;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Identifies FastAPI routes with deprecated uses of `Depends` or similar.
///
/// ## Why is this bad?
/// The [FastAPI documentation] recommends the use of [`typing.Annotated`] for
/// defining route dependencies and parameters, rather than using `Depends`,
/// `Query` or similar as a default value for a parameter. Using this approach
/// everywhere helps ensure consistency and clarity in defining dependencies
/// and parameters.
///
/// `Annotated` was added to the `typing` module in Python 3.9; however,
/// the third-party [`typing_extensions`] package provides a backport that can be
/// used on older versions of Python.
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
/// [fastAPI documentation]: https://fastapi.tiangolo.com/tutorial/query-params-str-validations/?h=annotated#advantages-of-annotated
/// [typing.Annotated]: https://docs.python.org/3/library/typing.html#typing.Annotated
/// [typing_extensions]: https://typing-extensions.readthedocs.io/en/stable/
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
        let title = if self.py_version >= PythonVersion::Py39 {
            "Replace with `typing.Annotated`"
        } else {
            "Replace with `typing_extensions.Annotated`"
        };
        Some(title.to_string())
    }
}

/// FAST002
pub(crate) fn fastapi_non_annotated_dependency(
    checker: &mut Checker,
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
        let needs_update = matches!(
            (&parameter.parameter.annotation, &parameter.default),
            (Some(_annotation), Some(default)) if is_fastapi_dependency(checker, default)
        );

        if needs_update {
            seen_default = create_diagnostic(checker, parameter, seen_default);
        } else if parameter.default.is_some() {
            seen_default = true;
        }
    }
}

fn is_fastapi_dependency(checker: &Checker, expr: &ast::Expr) -> bool {
    checker
        .semantic()
        .resolve_qualified_name(map_callable(expr))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                [
                    "fastapi",
                    "Query"
                        | "Path"
                        | "Body"
                        | "Cookie"
                        | "Header"
                        | "File"
                        | "Form"
                        | "Depends"
                        | "Security"
                ]
            )
        })
}

fn create_diagnostic(
    checker: &mut Checker,
    parameter: &ast::ParameterWithDefault,
    mut seen_default: bool,
) -> bool {
    let mut diagnostic = Diagnostic::new(
        FastApiNonAnnotatedDependency {
            py_version: checker.settings.target_version,
        },
        parameter.range,
    );

    if let (Some(annotation), Some(default)) = (&parameter.parameter.annotation, &parameter.default)
    {
        diagnostic.try_set_optional_fix(|| {
            let module = if checker.settings.target_version >= PythonVersion::Py39 {
                "typing"
            } else {
                "typing_extensions"
            };
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from(module, "Annotated"),
                parameter.range.start(),
                checker.semantic(),
            )?;

            // Refine the match from `is_fastapi_dependency` to exclude Depends
            // and Security, which don't have the same argument structure. The
            // others need to be converted from `q: str = Query("")` to `q:
            // Annotated[str, Query()] = ""` for example, but Depends and
            // Security need to stay like `Annotated[str, Depends(callable)]`
            let is_dep = checker
                .semantic()
                .resolve_qualified_name(map_callable(default))
                .is_some_and(|qualified_name| {
                    !matches!(
                        qualified_name.segments(),
                        ["fastapi", "Depends" | "Security"]
                    )
                });

            // Each of these classes takes a single, optional default
            // argument, followed by kw-only arguments
            let default_arg = default
                .as_call_expr()
                .and_then(|args| args.arguments.find_argument("default", 0));

            let kwarg_list: Option<Vec<_>> = default.as_call_expr().map(|args| {
                args.arguments
                    .keywords
                    .iter()
                    .filter_map(|kwarg| match kwarg.arg.as_ref() {
                        None => None,
                        Some(name) if name == "default" => None,
                        Some(_) => Some(checker.locator().slice(kwarg.range())),
                    })
                    .collect()
            });

            let content = match default_arg {
                Some(default_arg) if is_dep => {
                    let kwarg_list = match kwarg_list {
                        Some(v) => v.join(", "),
                        None => String::new(),
                    };
                    seen_default = true;
                    format!(
                        "{parameter_name}: {binding}[{annotation}, {default_}({kwarg_list})] \
                            = {default_value}",
                        parameter_name = &parameter.parameter.name.id,
                        annotation = checker.locator().slice(annotation.range()),
                        default_ = checker.locator().slice(map_callable(default).range()),
                        default_value = checker.locator().slice(default_arg.value().range()),
                    )
                }
                _ => {
                    if seen_default {
                        return Ok(None);
                    }
                    format!(
                        "{}: {}[{}, {}]",
                        parameter.parameter.name.id,
                        binding,
                        checker.locator().slice(annotation.range()),
                        checker.locator().slice(default.range())
                    )
                }
            };
            let parameter_edit = Edit::range_replacement(content, parameter.range);
            Ok(Some(Fix::unsafe_edits(import_edit, [parameter_edit])))
        });
    }

    checker.diagnostics.push(diagnostic);

    seen_default
}
