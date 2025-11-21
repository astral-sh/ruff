use crate::Violation;
use crate::checkers::ast::Checker;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::analyze::typing::{TypeChecker, check_type, traverse_union_and_optional};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks that async functions do not call blocking `os.path` or `pathlib.Path`
/// methods.
///
/// ## Why is this bad?
/// Calling some `os.path` or `pathlib.Path` methods in an async function will block
/// the entire event loop, preventing it from executing other tasks while waiting
/// for the operation. This negates the benefits of asynchronous programming.
///
/// Instead, use the methods' async equivalents from `trio.Path` or `anyio.Path`.
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
///
/// Non-blocking methods are OK to use:
/// ```python
/// import pathlib
///
///
/// async def func():
///     path = pathlib.Path("my_file.txt")
///     file_dirname = path.dirname()
///     new_path = os.path.join("/tmp/src/", path)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.13.2")]
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

/// ASYNC240
pub(crate) fn blocking_os_path(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();
    if !semantic.in_async_context() {
        return;
    }

    // Check if an expression is calling I/O related os.path method.
    // Just initializing pathlib.Path object is OK, we can return
    // early in that scenario.
    if let Some(qualified_name) = semantic.resolve_qualified_name(call.func.as_ref()) {
        let segments = qualified_name.segments();
        if !matches!(segments, ["os", "path", _]) {
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

    let Some(ast::ExprAttribute { value, attr, .. }) = call.func.as_attribute_expr() else {
        return;
    };

    if !maybe_calling_io_operation(attr.id.as_str()) {
        return;
    }

    // Check if an expression is a pathlib.Path constructor that directly
    // calls an I/O method.
    if PathlibPathChecker::match_initializer(value, semantic) {
        checker.report_diagnostic(
            BlockingPathMethodInAsyncFunction {
                path_library: "pathlib.Path".to_string(),
            },
            call.func.range(),
        );
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
        checker.report_diagnostic(
            BlockingPathMethodInAsyncFunction {
                path_library: "pathlib.Path".to_string(),
            },
            call.func.range(),
        );
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

fn maybe_calling_io_operation(attr: &str) -> bool {
    // ".open()" is added to the allow list to let ASYNC 230 handle
    // that case.
    !matches!(
        attr,
        "ALLOW_MISSING"
            | "altsep"
            | "anchor"
            | "as_posix"
            | "as_uri"
            | "basename"
            | "commonpath"
            | "commonprefix"
            | "curdir"
            | "defpath"
            | "devnull"
            | "dirname"
            | "drive"
            | "expandvars"
            | "extsep"
            | "genericpath"
            | "is_absolute"
            | "is_relative_to"
            | "is_reserved"
            | "isabs"
            | "join"
            | "joinpath"
            | "match"
            | "name"
            | "normcase"
            | "os"
            | "open"
            | "pardir"
            | "parent"
            | "parents"
            | "parts"
            | "pathsep"
            | "relative_to"
            | "root"
            | "samestat"
            | "sep"
            | "split"
            | "splitdrive"
            | "splitext"
            | "splitroot"
            | "stem"
            | "suffix"
            | "suffixes"
            | "supports_unicode_filenames"
            | "sys"
            | "with_name"
            | "with_segments"
            | "with_stem"
            | "with_suffix"
    )
}
