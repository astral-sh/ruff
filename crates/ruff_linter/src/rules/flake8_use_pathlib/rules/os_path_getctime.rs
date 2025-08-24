use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_path_getctime_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Checks for uses of `os.path.getctime`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`.
///
/// When possible, using `Path` object methods such as `Path.stat()` can
/// improve readability over the `os.path` module's counterparts (e.g.,
/// `os.path.getctime()`).
///
/// ## Example
/// ```python
/// import os
///
/// os.path.getctime(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).stat().st_ctime
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
/// - [Python documentation: `Path.stat`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.stat)
/// - [Python documentation: `os.path.getctime`](https://docs.python.org/3/library/os.path.html#os.path.getctime)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathGetctime;

impl Violation for OsPathGetctime {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.getctime` should be replaced by `Path.stat().st_ctime`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path.stat(...).st_ctime`".to_string())
    }
}

/// PTH205
pub(crate) fn os_path_getctime(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "getctime"] {
        return;
    }
    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "stat().st_ctime",
        "filename",
        is_fix_os_path_getctime_enabled(checker.settings()),
        OsPathGetctime,
    );
}
