use crate::Violation;
use crate::checkers::ast::Checker;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::analyze::typing::{TypeChecker, check_type, traverse_union_and_optional};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks that async functions do not use `os.path` or `pathlib.Path`.
///
/// ## Why is this bad?
/// Calling `os.path` or `pathlib.Path` methods in an async function will block
/// the entire event loop, preventing it from executing other tasks while waiting
/// for the operation. This negates the benefits of asynchronous programming.
///
/// Instead of `os.path` or `pathlib.Path`, use `trio.Path` or `anyio.path` objects.
///
/// ## Example
/// ```python
/// import os
///
///
/// async def func():
///     path = "my_file.txt"
///     file_exists = os.path.exists(path)
/// ```
///
/// Use instead:
/// ```python
/// import trio
///
///
/// async def func():
///     path = trio.Path("my_file.txt")
///     file_exists = await path.exists()
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BlockingPathMethodInAsyncFunction;

impl Violation for BlockingPathMethodInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not use pathlib.Path or os.path methods, use trio.Path or anyio.path".to_string()
    }
}

const OS_PATH: [&str; 2] = ["os", "path"];
const PATHLIB_PATH: [&str; 2] = ["pathlib", "Path"];

fn is_blocking_path_method(segments: &[&str]) -> bool {
    segments.starts_with(&OS_PATH) || segments.starts_with(&PATHLIB_PATH)
}

struct PathMethodInAsyncChecker;

impl TypeChecker for PathMethodInAsyncChecker {
    fn match_annotation(
        annotation: &ruff_python_ast::Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        // match base annotation directly
        if semantic
            .resolve_qualified_name(annotation)
            .is_some_and(|qualified_name| is_blocking_path_method(qualified_name.segments()))
        {
            return true;
        }

        // otherwise traverse any union or optional annotation
        let mut found = false;
        traverse_union_and_optional(
            &mut |inner_expr, _| {
                if semantic
                    .resolve_qualified_name(inner_expr)
                    .is_some_and(|qualified_name| {
                        is_blocking_path_method(qualified_name.segments())
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
            .is_some_and(|qualified_name| is_blocking_path_method(qualified_name.segments()))
    }
}

/// ASYNC240
pub(crate) fn blocking_os_path(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();
    if !semantic.in_async_context() {
        return;
    }

    // Check if an expression is directly calling one of the blocking `path`
    // objects.
    if let Some(qualified_name) = semantic.resolve_qualified_name(call.func.as_ref()) {
        let segments = qualified_name.segments();
        if is_blocking_path_method(segments) {
            checker.report_diagnostic(BlockingPathMethodInAsyncFunction, call.func.range());
        }
        return;
    }

    // Past this, we're checking if a variable contains one of the blocking
    // `path` objects.
    let Some(ast::ExprAttribute { value, .. }) = call.func.as_attribute_expr() else {
        return;
    };

    let Some(name) = value.as_name_expr() else {
        return;
    };

    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return;
    };

    if check_type::<PathMethodInAsyncChecker>(binding, semantic) {
        checker.report_diagnostic(BlockingPathMethodInAsyncFunction, call.func.range());
    }
}
