use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_unlink_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    check_os_pathlib_single_arg_calls, is_keyword_only_argument_non_default,
};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.unlink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.unlink()` can improve readability over the `os`
/// module's counterparts (e.g., `os.unlink()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.unlink("file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").unlink()
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
/// - [Python documentation: `Path.unlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.unlink)
/// - [Python documentation: `os.unlink`](https://docs.python.org/3/library/os.html#os.unlink)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsUnlink;

impl Violation for OsUnlink {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.unlink()` should be replaced by `Path.unlink()`".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).unlink()`".to_string())
    }
}

/// PTH108
pub(crate) fn os_unlink(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.unlink)
    // ```text
    //            0         1
    // os.unlink(path, *, dir_fd=None)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }

    if segments != ["os", "unlink"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "unlink()",
        "path",
        is_fix_os_unlink_enabled(checker.settings()),
        OsUnlink,
        Applicability::Safe,
    );
}
