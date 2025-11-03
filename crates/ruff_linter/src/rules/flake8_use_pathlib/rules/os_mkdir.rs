use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_mkdir_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_keyword_only_argument_non_default,
    is_pathlib_path_call,
};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.mkdir`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.mkdir()` can improve readability over the `os`
/// module's counterparts (e.g., `os.mkdir()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.mkdir("./directory/")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("./directory/").mkdir()
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
/// - [Python documentation: `os.mkdir`](https://docs.python.org/3/library/os.html#os.mkdir)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsMkdir;

impl Violation for OsMkdir {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.mkdir()` should be replaced by `Path.mkdir()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).mkdir()`".to_string())
    }
}

/// PTH102
pub(crate) fn os_mkdir(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "mkdir"] {
        return;
    }
    // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.mkdir)
    // ```text
    //           0     1                2
    // os.mkdir(path, mode=0o777, *, dir_fd=None)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }

    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(OsMkdir, call.func.range());

    let Some(path) = call.arguments.find_argument_value("path", 0) else {
        return;
    };

    if !is_fix_os_mkdir_enabled(checker.settings()) {
        return;
    }

    if call.arguments.len() > 2 {
        return;
    }

    if has_unknown_keywords_or_starred_expr(&call.arguments, &["path", "mode"]) {
        return;
    }

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

        let path_code = checker.locator().slice(path.range());

        let mkdir_args = call
            .arguments
            .find_argument_value("mode", 1)
            .map(|expr| format!("mode={}", checker.locator().slice(expr.range())))
            .unwrap_or_default();

        let replacement = if is_pathlib_path_call(checker, path) {
            format!("{path_code}.mkdir({mkdir_args})")
        } else {
            format!("{binding}({path_code}).mkdir({mkdir_args})")
        };

        Ok(Fix::applicable_edits(
            Edit::range_replacement(replacement, range),
            [import_edit],
            applicability,
        ))
    });
}
