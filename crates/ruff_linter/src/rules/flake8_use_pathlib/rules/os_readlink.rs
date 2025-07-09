use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_readlink_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    check_os_pathlib_single_arg_calls, is_keyword_only_argument_non_default,
};
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprCall, PythonVersion};

/// ## What it does
/// Checks for uses of `os.readlink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.readlink()` can improve readability over the `os`
/// module's counterparts (e.g., `os.readlink()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.readlink(file_name)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(file_name).readlink()
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
/// - [Python documentation: `Path.readlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.readline)
/// - [Python documentation: `os.readlink`](https://docs.python.org/3/library/os.html#os.readlink)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsReadlink;

impl Violation for OsReadlink {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.readlink()` should be replaced by `Path.readlink()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).readlink()`".to_string())
    }
}

/// PTH115
pub(crate) fn os_readlink(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    // Python 3.9+
    if checker.target_version() < PythonVersion::PY39 {
        return;
    }
    // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.readlink)
    // ```text
    //               0         1
    // os.readlink(path, *, dir_fd=None)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }

    if segments != ["os", "readlink"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "readlink()",
        "path",
        is_fix_os_readlink_enabled(checker.settings()),
        OsReadlink,
    );
}
