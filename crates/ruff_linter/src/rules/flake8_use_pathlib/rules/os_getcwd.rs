use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_getcwd_enabled;
use crate::{FixAvailability, Violation};
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `os.getcwd` and `os.getcwdb`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.cwd()` can improve readability over the `os`
/// module's counterparts (e.g., `os.getcwd()`).
///
/// ## Examples
/// ```python
/// import os
///
/// cwd = os.getcwd()
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// cwd = Path.cwd()
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
/// - [Python documentation: `Path.cwd`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.cwd)
/// - [Python documentation: `os.getcwd`](https://docs.python.org/3/library/os.html#os.getcwd)
/// - [Python documentation: `os.getcwdb`](https://docs.python.org/3/library/os.html#os.getcwdb)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsGetcwd;

impl Violation for OsGetcwd {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.getcwd()` should be replaced by `Path.cwd()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path.cwd()`".to_string())
    }
}

/// PTH109
pub(crate) fn os_getcwd(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if !matches!(segments, ["os", "getcwd" | "getcwdb"]) {
        return;
    }

    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(OsGetcwd, call.func.range());

    if !call.arguments.is_empty() {
        return;
    }

    if is_fix_os_getcwd_enabled(checker.settings()) {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("pathlib", "Path"),
                call.start(),
                checker.semantic(),
            )?;

            let applicability = if checker.comment_ranges().intersects(range) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };

            let replacement = format!("{binding}.cwd()");

            Ok(Fix::applicable_edits(
                Edit::range_replacement(replacement, range),
                [import_edit],
                applicability,
            ))
        });
    }
}
