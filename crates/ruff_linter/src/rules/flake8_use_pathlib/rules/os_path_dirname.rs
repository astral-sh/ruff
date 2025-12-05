use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_path_dirname_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.path.dirname`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.parent` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.dirname()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.dirname(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).parent
/// ```
///
/// ## Fix Safety
/// This rule's fix is always marked as unsafe because the replacement is not always semantically
/// equivalent to the original code. In particular, `pathlib` performs path normalization,
/// which can alter the result compared to `os.path.dirname`. For example, this normalization:
///
/// - Collapses consecutive slashes (e.g., `"a//b"` → `"a/b"`).
/// - Removes trailing slashes (e.g., `"a/b/"` → `"a/b"`).
/// - Eliminates `"."` (e.g., `"a/./b"` → `"a/b"`).
///
/// As a result, code relying on the exact string returned by `os.path.dirname`
/// may behave differently after the fix.
///
/// Additionally, the fix is marked as unsafe because `os.path.dirname()` returns `str` or `bytes` (`AnyStr`),
/// while `Path.parent` returns a `Path` object. This change in return type can break code that uses
/// the return value.
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `PurePath.parent`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.parent)
/// - [Python documentation: `os.path.dirname`](https://docs.python.org/3/library/os.path.html#os.path.dirname)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsPathDirname;

impl Violation for OsPathDirname {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.dirname()` should be replaced by `Path.parent`".to_string()
    }
    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).parent`".to_string())
    }
}

/// PTH120
pub(crate) fn os_path_dirname(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "dirname"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "parent",
        "p",
        is_fix_os_path_dirname_enabled(checker.settings()),
        OsPathDirname,
        Applicability::Unsafe,
    );
}
