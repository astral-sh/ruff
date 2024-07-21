use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
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

impl AlwaysFixableViolation for FastApiNonAnnotatedDependency {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("FastAPI dependency without `Annotated`")
    }

    fn fix_title(&self) -> String {
        "Replace with `Annotated`".to_string()
    }
}

/// RUF103
pub(crate) fn fastapi_non_annotated_dependency(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }
    if !is_fastapi_route(function_def, checker.semantic()) {
        return;
    }
    for parameter in &function_def.parameters.args {
        if let (Some(annotation), Some(default)) =
            (&parameter.parameter.annotation, &parameter.default)
        {
            if checker
                .semantic()
                .resolve_qualified_name(map_callable(default))
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
            {
                let mut diagnostic =
                    Diagnostic::new(FastApiNonAnnotatedDependency, parameter.range);

                diagnostic.try_set_fix(|| {
                    let module = if checker.settings.target_version >= PythonVersion::Py39 {
                        "typing"
                    } else {
                        "typing_extensions"
                    };
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import_from(module, "Annotated"),
                        function_def.start(),
                        checker.semantic(),
                    )?;
                    let content = format!(
                        "{}: {}[{}, {}]",
                        parameter.parameter.name.id,
                        binding,
                        checker.locator().slice(annotation.range()),
                        checker.locator().slice(default.range())
                    );
                    let parameter_edit = Edit::range_replacement(content, parameter.range());
                    Ok(Fix::unsafe_edits(import_edit, [parameter_edit]))
                });

                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
