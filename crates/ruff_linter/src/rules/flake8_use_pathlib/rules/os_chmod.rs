use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ArgOrKeyword, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_chmod_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_file_descriptor, is_keyword_only_argument_non_default,
    is_pathlib_path_call,
};
use crate::{FixAvailability, Violation};

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
#[violation_metadata(stable_since = "v0.0.231")]
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
    let path_arg = call.arguments.find_argument_value("path", 0);

    if path_arg.is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
        || is_keyword_only_argument_non_default(&call.arguments, "dir_fd")
    {
        return;
    }

    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(OsChmod, call.func.range());

    if !is_fix_os_chmod_enabled(checker.settings()) {
        return;
    }

    if call.arguments.len() < 2 {
        return;
    }

    if has_unknown_keywords_or_starred_expr(
        &call.arguments,
        &["path", "mode", "dir_fd", "follow_symlinks"],
    ) {
        return;
    }

    let (Some(path_arg), Some(_)) = (path_arg, call.arguments.find_argument_value("mode", 1))
    else {
        return;
    };

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let locator = checker.locator();
        let path_code = locator.slice(path_arg.range());

        let args = |arg: ArgOrKeyword| match arg {
            ArgOrKeyword::Arg(expr) if expr.range() != path_arg.range() => {
                Some(locator.slice(expr.range()))
            }
            ArgOrKeyword::Keyword(kw)
                if matches!(kw.arg.as_deref(), Some("mode" | "follow_symlinks")) =>
            {
                Some(locator.slice(kw.range()))
            }
            _ => None,
        };

        let chmod_args = itertools::join(
            call.arguments.arguments_source_order().filter_map(args),
            ", ",
        );

        let replacement = if is_pathlib_path_call(checker, path_arg) {
            format!("{path_code}.chmod({chmod_args})")
        } else {
            format!("{binding}({path_code}).chmod({chmod_args})")
        };

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
