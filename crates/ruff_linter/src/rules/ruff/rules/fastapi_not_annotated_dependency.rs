use crate::checkers::ast::Checker;
use crate::rules::ruff::fastapi::is_fastapi_route;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::Modules;

/// ## What it does
/// Identifies FastApi routes using the deprecated dependency style, which omits Annotated.
///
/// ## Why is this bad?
/// The FastApi documentation recommends employing Annotated for dependencies.
/// This approach is also suggested for various route parameters, including Body and Cookie, as it helps ensure consistency and clarity in defining dependencies and parameters.
/// By following these guidelines, developers can create more readable and maintainable FastApi applications.
/// For more information, see the [FastAPI documentation](https://fastapi.tiangolo.com/tutorial/dependencies/#dependencies).
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
///
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
pub struct FastApiNotAnnotatedDependency {
    name: String,
}

impl Violation for FastApiNotAnnotatedDependency {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("FastAPI dependency without Annotated")
    }

    // fn fix_title(&self) -> String {
    //     format!("Parameter {} should use Annotated.", self.name)
    //
    // }
}

/// RUF103
pub(crate) fn fastapi_not_annotated_dependency(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }
    if !is_fastapi_route(checker, function_def) {
        return;
    }
    for parameter in &function_def.parameters.args {
        if let (Some(_), Some(default)) = (&parameter.parameter.annotation, &parameter.default) {
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
                let diagnostic = Diagnostic::new(
                    FastApiNotAnnotatedDependency {
                        name: parameter.parameter.name.id.to_string(),
                    },
                    parameter.range,
                );
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
