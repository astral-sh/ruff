use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_rename_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    check_os_pathlib_two_arg_calls, has_unknown_keywords_or_starred_expr,
    is_keyword_only_argument_non_default, is_top_level_expression_call,
};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.rename`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.rename()` can improve readability over the `os`
/// module's counterparts (e.g., `os.rename()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.rename("old.py", "new.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("old.py").rename("new.py")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the original expression.
/// Additionally, the fix is marked as unsafe when the return value is used because the type changes
/// from `None` to a `Path` object.
///
/// ## References
/// - [Python documentation: `Path.rename`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rename)
/// - [Python documentation: `os.rename`](https://docs.python.org/3/library/os.html#os.rename)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsRename;

impl Violation for OsRename {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.rename()` should be replaced by `Path.rename()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).rename(...)`".to_string())
    }
}

/// PTH104
pub(crate) fn os_rename(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "rename"] {
        return;
    }
    // `src_dir_fd` and `dst_dir_fd` are not supported by pathlib, so check if they are
    // set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.rename)
    // ```text
    //            0    1           2                  3
    // os.rename(src, dst, *, src_dir_fd=None, dst_dir_fd=None)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "src_dir_fd")
        || is_keyword_only_argument_non_default(&call.arguments, "dst_dir_fd")
    {
        return;
    }

    let fix_enabled = is_fix_os_rename_enabled(checker.settings())
        && !has_unknown_keywords_or_starred_expr(
            &call.arguments,
            &["src", "dst", "src_dir_fd", "dst_dir_fd"],
        );

    // Unsafe when the fix would delete comments or change a used return value
    let applicability = if !is_top_level_expression_call(checker) {
        // Unsafe because the return type changes (None -> Path)
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    check_os_pathlib_two_arg_calls(
        checker,
        call,
        "rename",
        "src",
        "dst",
        fix_enabled,
        OsRename,
        applicability,
    );
}
