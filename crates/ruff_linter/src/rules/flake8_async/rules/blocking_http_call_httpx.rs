use ruff_python_ast::{self as ast, Expr, ExprCall};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::analyze::typing::{TypeChecker, check_type, traverse_union_and_optional};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not use blocking httpx clients.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking HTTP call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// HTTP response, negating the benefits of asynchronous programming.
///
/// Instead of using the blocking `httpx` client, use the asynchronous client.
///
/// ## Example
/// ```python
/// import httpx
///
///
/// async def fetch():
///     client = httpx.Client()
///     response = client.get(...)
/// ```
///
/// Use instead:
/// ```python
/// import httpx
///
///
/// async def fetch():
///     async with httpx.AsyncClient() as client:
///         response = await client.get(...)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BlockingHttpCallHttpxInAsyncFunction;

impl Violation for BlockingHttpCallHttpxInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not use blocking httpx clients".to_string()
    }
}

struct HttpxClientChecker;

impl TypeChecker for HttpxClientChecker {
    fn match_annotation(
        annotation: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        let mut found = false;
        traverse_union_and_optional(
            &mut |inner_expr, _| {
                if semantic
                    .resolve_qualified_name(inner_expr)
                    .is_some_and(|qualified_name| {
                        matches!(qualified_name.segments(), ["httpx", "Client"])
                    })
                {
                    found = true;
                }
            },
            semantic,
            annotation,
        );
        found
    }

    fn match_initializer(
        initializer: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        let Expr::Call(ExprCall { func, .. }) = initializer else {
            return false;
        };

        semantic
            .resolve_qualified_name(func)
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["httpx", "Client"]))
    }
}

/// ASYNC212
pub(crate) fn blocking_http_call_httpx(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();
    if semantic.in_async_context() {
        if let Some(ast::ExprAttribute { value, attr, .. }) = call.func.as_attribute_expr() {
            if let Some(name) = value.as_name_expr() {
                if let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) {
                    if check_type::<HttpxClientChecker>(binding, semantic) {
                        if matches!(
                            attr.id.as_str(),
                            "close"
                                | "delete"
                                | "get"
                                | "head"
                                | "options"
                                | "patch"
                                | "post"
                                | "put"
                                | "request"
                                | "send"
                                | "stream"
                        ) {
                            checker.report_diagnostic(
                                BlockingHttpCallHttpxInAsyncFunction,
                                call.func.range(),
                            );
                        }
                    }
                }
            }
        }
    }
}
