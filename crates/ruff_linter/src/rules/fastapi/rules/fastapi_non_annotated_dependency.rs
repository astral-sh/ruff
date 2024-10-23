use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::fastapi::rules::is_fastapi_route;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Identifies FastAPI routes with deprecated uses of `Depends`.
///
/// ## Why is this bad?
/// The FastAPI documentation recommends the use of `Annotated` for defining
/// route dependencies and parameters, rather than using `Depends` directly
/// with a default value.
///
/// This approach is also suggested for various route parameters, including Body and Cookie, as it helps ensure consistency and clarity in defining dependencies and parameters.
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

#[violation]
pub struct FastApiNonAnnotatedDependency;

impl Violation for FastApiNonAnnotatedDependency {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("FastAPI dependency without `Annotated`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Annotated`".to_string())
    }
}

/// RUF103
pub(crate) fn fastapi_non_annotated_dependency(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI)
        || !is_fastapi_route(function_def, checker.semantic())
    {
        return;
    }

    let mut updatable_count = 0;
    let mut has_non_updatable_default = false;
    let total_params = function_def.parameters.args.len();

    for parameter in &function_def.parameters.args {
        let needs_update = matches!(
            (&parameter.parameter.annotation, &parameter.default),
            (Some(_annotation), Some(default)) if is_fastapi_dependency(checker, default)
        );

        if needs_update {
            updatable_count += 1;
            // Determine if it's safe to update this parameter:
            // - if all parameters are updatable its safe.
            // - if we've encountered a non-updatable parameter with a default value, it's no longer
            //   safe. (https://github.com/astral-sh/ruff/issues/12982)
            let safe_to_update = updatable_count == total_params || !has_non_updatable_default;
            create_diagnostic(checker, parameter, safe_to_update);
        } else if parameter.default.is_some() {
            has_non_updatable_default = true;
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
    safe_to_update: bool,
) {
    let mut diagnostic = Diagnostic::new(FastApiNonAnnotatedDependency, parameter.range);

    if safe_to_update {
        if let (Some(annotation), Some(default)) =
            (&parameter.parameter.annotation, &parameter.default)
        {
            diagnostic.try_set_fix(|| {
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
                let content = format!(
                    "{}: {}[{}, {}]",
                    parameter.parameter.name.id,
                    binding,
                    checker.locator().slice(annotation.range()),
                    checker.locator().slice(default.range())
                );
                let parameter_edit = Edit::range_replacement(content, parameter.range);
                Ok(Fix::unsafe_edits(import_edit, [parameter_edit]))
            });
        }
    } else {
        diagnostic.fix = None;
    }

    checker.diagnostics.push(diagnostic);
}
