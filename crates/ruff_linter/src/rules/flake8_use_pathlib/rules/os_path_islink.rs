use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_path_islink_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Checks for uses of `os.path.islink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.is_symlink()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.islink()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.islink("docs")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("docs").is_symlink()
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
/// - [Python documentation: `Path.is_symlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_symlink)
/// - [Python documentation: `os.path.islink`](https://docs.python.org/3/library/os.path.html#os.path.islink)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathIslink;

impl Violation for OsPathIslink {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.islink()` should be replaced by `Path.is_symlink()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).is_symlink()`".to_string())
    }
}

/// PTH114
pub(crate) fn os_path_islink(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "islink"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "is_symlink()",
        "path",
        is_fix_os_path_islink_enabled(checker.settings()),
        OsPathIslink,
    );
}
