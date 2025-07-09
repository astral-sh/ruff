use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_path_isabs_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Checks for uses of `os.path.isabs`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.is_absolute()` can improve readability over the `os.path`
/// module's counterparts (e.g.,  as `os.path.isabs()`).
///
/// ## Examples
/// ```python
/// import os
///
/// if os.path.isabs(file_name):
///     print("Absolute path!")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// if Path(file_name).is_absolute():
///     print("Absolute path!")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `PurePath.is_absolute`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.is_absolute)
/// - [Python documentation: `os.path.isabs`](https://docs.python.org/3/library/os.path.html#os.path.isabs)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathIsabs;

impl Violation for OsPathIsabs {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.isabs()` should be replaced by `Path.is_absolute()`".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).is_absolute()`".to_string())
    }
}

/// PTH117
pub(crate) fn os_path_isabs(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "isabs"] {
        return;
    }
    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "is_absolute()",
        "s",
        is_fix_os_path_isabs_enabled(checker.settings()),
        OsPathIsabs,
    );
}
