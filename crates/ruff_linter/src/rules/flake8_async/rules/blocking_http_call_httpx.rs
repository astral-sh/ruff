use ruff_python_ast::{self as ast, Expr, ExprCall};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::analyze::typing::{TypeChecker, check_type, traverse_union_and_optional};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks that async functions do not use blocking httpx/httpx2 clients.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking HTTP call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// HTTP response, negating the benefits of asynchronous programming.
///
/// Instead of using the blocking `httpx`/`httpx2` client, use the asynchronous client.
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
#[violation_metadata(stable_since = "0.15.0", category = Category::Suspicious)]
pub(crate) struct BlockingHttpCallHttpxInAsyncFunction {
    package: HttpxPackage,
    name: String,
    call: String,
}

impl Violation for BlockingHttpCallHttpxInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Blocking {package} method {name}.{call}() in async context, use {package}.AsyncClient",
            package = self.package.name(),
            name = self.name,
            call = self.call,
        )
    }
}
#[derive(Clone, Copy)]
enum HttpxPackage {
    Httpx,
    Httpx2,
}

impl HttpxPackage {
    fn name(self) -> &'static str {
        match self {
            HttpxPackage::Httpx => "httpx",
            HttpxPackage::Httpx2 => "httpx2",
        }
    }
}

fn match_annotation(
    package: HttpxPackage,
    annotation: &ruff_python_ast::Expr,
    semantic: &ruff_python_semantic::SemanticModel,
) -> bool {
    // match base annotation directly
    if semantic.matches_qualified_name(annotation, &[package.name(), "Client"]) {
        return true;
    }

    // otherwise traverse any union or optional annotation
    let mut found = false;
    traverse_union_and_optional(
        &mut |inner_expr, _| {
            if semantic.matches_qualified_name(inner_expr, &[package.name(), "Client"]) {
                found = true;
            }
        },
        semantic,
        annotation,
    );
    found
}

fn match_initializer(
    package: HttpxPackage,
    initializer: &ruff_python_ast::Expr,
    semantic: &ruff_python_semantic::SemanticModel,
) -> bool {
    let Expr::Call(ExprCall { func, .. }) = initializer else {
        return false;
    };

    semantic.matches_qualified_name(func, &[package.name(), "Client"])
}

struct HttpxClientChecker;

impl TypeChecker for HttpxClientChecker {
    fn match_annotation(
        annotation: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        match_annotation(HttpxPackage::Httpx, annotation, semantic)
    }

    fn match_initializer(
        initializer: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        match_initializer(HttpxPackage::Httpx, initializer, semantic)
    }
}

struct Httpx2ClientChecker;

impl TypeChecker for Httpx2ClientChecker {
    fn match_annotation(
        annotation: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        match_annotation(HttpxPackage::Httpx2, annotation, semantic)
    }

    fn match_initializer(
        initializer: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        match_initializer(HttpxPackage::Httpx2, initializer, semantic)
    }
}

/// ASYNC212
pub(crate) fn blocking_http_call_httpx(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();
    if !semantic.in_async_context() {
        return;
    }

    let Some(ast::ExprAttribute { value, attr, .. }) = call.func.as_attribute_expr() else {
        return;
    };
    let Some(name) = value.as_name_expr() else {
        return;
    };
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return;
    };
    let package = if check_type::<HttpxClientChecker>(binding, semantic) {
        HttpxPackage::Httpx
    } else if check_type::<Httpx2ClientChecker>(binding, semantic) {
        HttpxPackage::Httpx2
    } else {
        return;
    };

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
            BlockingHttpCallHttpxInAsyncFunction {
                package,
                name: name.id.to_string(),
                call: attr.id.to_string(),
            },
            call.func.range(),
        );
    }
}
