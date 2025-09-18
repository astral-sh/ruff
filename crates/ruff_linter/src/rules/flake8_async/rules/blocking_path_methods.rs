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
pub(crate) struct BlockingPathMethodInAsyncFunction {
    path_library: String,
}

impl Violation for BlockingPathMethodInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Async functions should not use {path_library} methods, use trio.Path or anyio.path",
            path_library = self.path_library
        )
    }
}

struct PathlibPathChecker;

impl PathlibPathChecker {
    fn is_pathlib_path_constructor(
        semantic: &ruff_python_semantic::SemanticModel,
        expr: &Expr,
    ) -> bool {
        let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
            return false;
        };

        matches!(
            qualified_name.segments(),
            [
                "pathlib",
                "Path"
                    | "PosixPath"
                    | "PurePath"
                    | "PurePosixPath"
                    | "PureWindowsPath"
                    | "WindowsPath"
            ]
        )
    }
}

impl TypeChecker for PathlibPathChecker {
    fn match_annotation(annotation: &Expr, semantic: &ruff_python_semantic::SemanticModel) -> bool {
        if Self::is_pathlib_path_constructor(semantic, annotation) {
            return true;
        }

        let mut found = false;
        traverse_union_and_optional(
            &mut |inner_expr, _| {
                if Self::is_pathlib_path_constructor(semantic, inner_expr) {
                    found = true;
                }
            },
            semantic,
            annotation,
        );
        found
    }

    fn match_initializer(
        initializer: &Expr,
        semantic: &ruff_python_semantic::SemanticModel,
    ) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };

        Self::is_pathlib_path_constructor(semantic, func)
    }
}

fn is_calling_os_path_method(segments: &[&str]) -> bool {
    if segments.len() != 3 {
        return false;
    }
    let Some(symbol_name) = segments.get(..2) else {
        return false;
    };
    matches!(symbol_name, ["os", "path"])
}

fn maybe_calling_io_operation(attr: &str) -> bool {
    !matches!(
        attr,
        // Pure path objects provide path-handling operations which don’t actually
        // access a filesystem.
        // https://docs.python.org/3/library/pathlib.html#pure-paths
        // pathlib.PurePath methods and properties:
        "anchor"
            | "as_posix"
            | "as_uri"
            | "drive"
            | "is_absolute"
            | "is_relative_to"
            | "is_reserved"
            | "joinpath"
            | "match"
            | "name"
            | "parent"
            | "parents"
            | "parts"
            | "relative_to"
            | "root"
            | "stem"
            | "suffix"
            | "suffixes"
            | "with_name"
            | "with_segments"
            | "with_stem"
            | "with_suffix"
            // Non I/O pathlib.Path or os.path methods:
            | "join"
            | "dirname"
            | "basename"
            | "splitroot"
            | "splitdrive"
            | "splitext"
            | "split"
            | "isabs"
            | "normcase"
    )
}

/// ASYNC240
pub(crate) fn blocking_os_path(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();
    if !semantic.in_async_context() {
        return;
    }

    // Check if an expression is calling I/O related os.path method.
    // Just intializing pathlib.Path object is OK, we can return
    // early in that scenario.
    if let Some(qualified_name) = semantic.resolve_qualified_name(call.func.as_ref()) {
        let segments = qualified_name.segments();
        if !is_calling_os_path_method(segments) {
            return;
        }

        let Some(os_path_method) = segments.last() else {
            return;
        };

        if maybe_calling_io_operation(os_path_method) {
            checker.report_diagnostic(
                BlockingPathMethodInAsyncFunction {
                    path_library: "os.path".to_string(),
                },
                call.func.range(),
            );
        }
        return;
    }

    // Check if an expression is a pathlib.Path constructor that directly
    // calls an I/O method.
    let Some(ast::ExprAttribute { value, attr, .. }) = call.func.as_attribute_expr() else {
        return;
    };

    if let Some(ExprCall { func, .. }) = value.as_call_expr() {
        if !PathlibPathChecker::is_pathlib_path_constructor(semantic, func) {
            return;
        }
        if maybe_calling_io_operation(attr.id.as_str()) {
            checker.report_diagnostic(
                BlockingPathMethodInAsyncFunction {
                    path_library: "pathlib.Path".to_string(),
                },
                call.func.range(),
            );
        }
        return;
    }

    // Lastly, check if a variable is a pathlib.Path instance and it's
    // calling an I/O method.
    let Some(name) = value.as_name_expr() else {
        return;
    };

    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return;
    };

    if check_type::<PathlibPathChecker>(binding, semantic) {
        if maybe_calling_io_operation(attr.id.as_str()) {
            checker.report_diagnostic(
                BlockingPathMethodInAsyncFunction {
                    path_library: "pathlib.Path".to_string(),
                },
                call.func.range(),
            );
        }
    }
}
