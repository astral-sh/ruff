use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_path_basename_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.path.basename`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.name` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.basename()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.basename(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).name
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## Fix Safety
/// This rule's fix is always marked as unsafe because the replacement is not always semantically
/// equivalent to the original code. In particular, `pathlib` performs path normalization,
/// which can alter the result compared to `os.path.basename`. For example, this normalization:
///
/// - Collapses consecutive slashes (e.g., `"a//b"` → `"a/b"`).
/// - Removes trailing slashes (e.g., `"a/b/"` → `"a/b"`).
/// - Eliminates `"."` (e.g., `"a/./b"` → `"a/b"`).
///
/// As a result, code relying on the exact string returned by `os.path.basename`
/// may behave differently after the fix.
///
/// ## References
/// - [Python documentation: `PurePath.name`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.name)
/// - [Python documentation: `os.path.basename`](https://docs.python.org/3/library/os.path.html#os.path.basename)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsPathBasename;

impl Violation for OsPathBasename {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.basename()` should be replaced by `Path.name`".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).name`".to_string())
    }
}

/// PTH119
pub(crate) fn os_path_basename(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "basename"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "name",
        "p",
        is_fix_os_path_basename_enabled(checker.settings()),
        OsPathBasename,
        Applicability::Unsafe,
    );
}
