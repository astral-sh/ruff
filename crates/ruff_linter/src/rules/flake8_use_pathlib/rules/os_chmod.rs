use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_chmod_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    check_os_pathlib_two_arg_calls, is_file_descriptor, is_keyword_only_argument_non_default,
};
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Checks for uses of `os.chmod`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.chmod()` can improve readability over the `os`
/// module's counterparts (e.g., `os.chmod()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.chmod("file.py", 0o444)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").chmod(0o444)
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the original expression.
///
/// ## References
/// - [Python documentation: `Path.chmod`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.chmod)
/// - [Python documentation: `os.chmod`](https://docs.python.org/3/library/os.html#os.chmod)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsChmod;

impl Violation for OsChmod {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.chmod()` should be replaced by `Path.chmod()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).chmod(...)`".to_string())
    }
}

/// PTH101
pub(crate) fn os_chmod(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "chmod"] {
        return;
    }

    // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.chmod)
    // ```text
    //           0     1          2               3
    // os.chmod(path, mode, *, dir_fd=None, follow_symlinks=True)
    // ```
    if call
        .arguments
        .find_argument_value("path", 0)
        .is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
        || is_keyword_only_argument_non_default(&call.arguments, "dir_fd")
    {
        return;
    }

    check_os_pathlib_two_arg_calls(
        checker,
        call,
        "chmod",
        "path",
        "mode",
        is_fix_os_chmod_enabled(checker.settings()),
        OsChmod,
    );
}
