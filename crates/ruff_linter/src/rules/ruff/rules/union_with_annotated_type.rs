use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprSubscript};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Annotated[]` types within a `Union` or `Optional` type.
///
/// ## Why is this bad?
/// Consumers of `Annotated` types often only check the top-level type for annotations,
/// and may miss `Annotated` types inside other types, such as `Optional` or `Union`
///
/// ```python
/// from typing import Annotated, get_type_hints
///
/// def f(a: Annotated[str, "test data"]): ...
/// def z(a: Annotated[str, "test data"] | None): ...
/// def b(a: Annotated[str | None, "test data"]): ...
///
/// get_type_hints(f, include_extras=True)
/// # {'a': typing.Annotated[str, 'test data']}
/// get_type_hints(z, include_extras=True)
/// # {'a': typing.Optional[typing.Annotated[str, 'test data']]}
/// get_type_hints(b, include_extras=True)
/// # {'a': typing.Annotated[str | None, 'test data']}
/// ```
///
/// ## Example
/// ```python
/// from typing import Annotated, Optional
/// from fastapi import FastAPI, Query
///
/// app = FastAPI()
///
/// @app.get('/route')
/// def route(param: Annotated[str, Query()] | None = None):
///    ...
/// ```
/// This fails to parse `param` as a query parameter. Use instead:
/// ```python
/// @app.get('/route')
/// def route(param: Annotated[str | None, Query()] = None):
///   ...
/// ```
///
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.4")]
pub(crate) struct UnionWithAnnotatedType {
    parent_type: ParentType,
}

impl Violation for UnionWithAnnotatedType {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.parent_type {
            ParentType::Subscript => {
                "`Annotated[]` type must not be part of a Union or Optional type".to_string()
            }
            ParentType::BinOp => {
                "`Annotated[]` type must not be part of a PEP604 type union (|)".to_string()
            }
        }
    }
}

/// RUF066
pub(crate) fn union_with_annotated_type(checker: &Checker, subscript: &ExprSubscript) {
    let semantic = checker.semantic();

    if !semantic.match_typing_expr(&subscript.value, "Annotated") {
        return;
    }

    let result = semantic
        .current_expressions()
        .skip(1)
        .filter_map(|expr| match expr {
            Expr::Subscript(_) => Some((expr, ParentType::Subscript)),
            Expr::BinOp(_) => Some((expr, ParentType::BinOp)),
            _ => None,
        })
        .last();

    if let Some((parent, parent_type)) = result {
        checker.report_diagnostic(UnionWithAnnotatedType { parent_type }, parent.range());
    }
}

enum ParentType {
    Subscript,
    BinOp,
}
