use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_replace_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    check_os_pathlib_two_arg_calls, is_keyword_only_argument_non_default,
};
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Checks for uses of `os.replace`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.replace()` can improve readability over the `os`
/// module's counterparts (e.g., `os.replace()`).
///
/// Note that `os` functions may be preferable if performance is a concern,
/// e.g., in hot loops.
///
/// ## Examples
/// ```python
/// import os
///
/// os.replace("old.py", "new.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("old.py").replace("new.py")
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
/// - [Python documentation: `Path.replace`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.replace)
/// - [Python documentation: `os.replace`](https://docs.python.org/3/library/os.html#os.replace)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsReplace;

impl Violation for OsReplace {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.replace()` should be replaced by `Path.replace()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).replace(...)`".to_string())
    }
}

/// PTH105
pub(crate) fn os_replace(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "replace"] {
        return;
    }
    // `src_dir_fd` and `dst_dir_fd` are not supported by pathlib, so check if they are
    // set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.replace)
    // ```text
    //             0    1           2                3
    // os.replace(src, dst, *, src_dir_fd=None, dst_dir_fd=None)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "src_dir_fd")
        || is_keyword_only_argument_non_default(&call.arguments, "dst_dir_fd")
    {
        return;
    }

    check_os_pathlib_two_arg_calls(
        checker,
        call,
        "replace",
        "src",
        "dst",
        is_fix_os_replace_enabled(checker.settings()),
        OsReplace,
    );
}
