use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_chmod_enabled;
use crate::rules::flake8_use_pathlib::helpers::is_pathlib_path_call;
use crate::rules::flake8_use_pathlib::helpers::{
    check_os_pathlib_two_arg_calls, collect_follow_symlinks, has_extra_positional_args,
    has_unknown_keywords_or_starred_expr, is_file_descriptor, is_keyword_only_argument_non_default,
};
use crate::{FixAvailability, Violation};
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `os.chmod`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.chmod()` can improve readability over the `os`
/// module's counterparts (e.g., `os.chmod()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.chmod("file.py", 0o444)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").chmod(0o444)
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
/// - [Python documentation: `Path.chmod`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.chmod)
/// - [Python documentation: `os.chmod`](https://docs.python.org/3/library/os.html#os.chmod)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsChmod;

impl Violation for OsChmod {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.chmod()` should be replaced by `Path.chmod()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).chmod(...)`".to_string())
    }
}

/// PTH101
pub(crate) fn os_chmod(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "chmod"] {
        return;
    }

    // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.chmod)
    // ```text
    //           0     1          2               3
    // os.chmod(path, mode, *, dir_fd=None, follow_symlinks=True)
    // ```
    if call
        .arguments
        .find_argument_value("path", 0)
        .is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
        || is_keyword_only_argument_non_default(&call.arguments, "dir_fd")
    {
        return;
    }

    // Suppress fix if unexpected starred args/keywords or too many positional args are present.
    // Allowed keywords here include `dir_fd` and `follow_symlinks`, but the former is already
    // filtered out above; we still include it here to treat any non-default presence as handled.
    if has_unknown_keywords_or_starred_expr(
        &call.arguments,
        &["path", "mode", "dir_fd", "follow_symlinks"],
    ) {
        return;
    }
    if has_extra_positional_args(&call.arguments, 1) {
        return;
    }

    // If follow_symlinks is explicitly provided, ensure we preserve it in the replacement.
    if let Some(follow) = collect_follow_symlinks(&call.arguments) {
        // Rebuild a custom two-arg call with keyword forwarding for follow_symlinks.
        let range = call.range();
        let Some(path_expr) = call.arguments.find_argument_value("path", 0) else {
            return;
        };
        let Some(mode_expr) = call.arguments.find_argument_value("mode", 1) else {
            return;
        };
        let path_code = checker.locator().slice(path_expr.range());
        let mode_code = checker.locator().slice(mode_expr.range());
        let follow_code = checker.locator().slice(follow.range());

        let mut diagnostic = checker.report_diagnostic(OsChmod, call.func.range());
        if is_fix_os_chmod_enabled(checker.settings()) {
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import("pathlib", "Path"),
                    call.start(),
                    checker.semantic(),
                )?;

                let receiver = if is_pathlib_path_call(checker, path_expr) {
                    path_code.to_string()
                } else {
                    format!("{binding}({path_code})")
                };

                let replacement =
                    format!("{receiver}.chmod({mode_code}, follow_symlinks={follow_code})");
                let applicability = if checker.comment_ranges().intersects(range) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };
                Ok(Fix::applicable_edits(
                    Edit::range_replacement(replacement, range),
                    [import_edit],
                    applicability,
                ))
            });
        }
    } else {
        check_os_pathlib_two_arg_calls(
            checker,
            call,
            "chmod",
            "path",
            "mode",
            is_fix_os_chmod_enabled(checker.settings()),
            OsChmod,
        );
    }
}
