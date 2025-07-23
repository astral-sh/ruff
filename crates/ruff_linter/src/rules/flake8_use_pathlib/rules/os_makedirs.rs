use crate::checkers::ast::Checker;
use crate::preview::is_fix_os_makedirs_enabled;
use crate::rules::flake8_use_pathlib::helpers::check_os_pathlib_single_arg_calls;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Checks for uses of `os.makedirs`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.mkdir(parents=True)` can improve readability over the
/// `os` module's counterparts (e.g., `os.makedirs()`.
///
/// ## Examples
/// ```python
/// import os
///
/// os.makedirs("./nested/directory/")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("./nested/directory/").mkdir(parents=True)
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
/// - [Python documentation: `Path.mkdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.mkdir)
/// - [Python documentation: `os.makedirs`](https://docs.python.org/3/library/os.html#os.makedirs)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsMakedirs;

impl Violation for OsMakedirs {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.makedirs()` should be replaced by `Path.mkdir(parents=True)`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).mkdir(parents=True)`".to_string())
    }
}

/// PTH103
pub(crate) fn os_makedirs(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "makedirs"] {
        return;
    }

    check_os_pathlib_single_arg_calls(
        checker,
        call,
        "mkdir(parents=True)",
        "name",
        is_fix_os_makedirs_enabled(checker.settings()),
        OsMakedirs,
    );
}
