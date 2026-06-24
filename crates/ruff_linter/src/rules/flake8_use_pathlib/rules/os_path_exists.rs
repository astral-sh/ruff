use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_path_exists_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.path.exists`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.exists()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.exists()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.exists("file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").exists()
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
/// - [Python documentation: `Path.exists`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.exists)
/// - [Python documentation: `os.path.exists`](https://docs.python.org/3/library/os.path.html#os.path.exists)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsPathExists;

impl Violation for OsPathExists {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.exists()` should be replaced by `Path.exists()`".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).exists()`".to_string())
    }
}

/// PTH110
pub(crate) fn os_path_exists(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "exists"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "exists()",
        "path",
        is_fix_os_path_exists_enabled(checker.settings()),
        OsPathExists,
        Applicability::Safe,
    );
}
