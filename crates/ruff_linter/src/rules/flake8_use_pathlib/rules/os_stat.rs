use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ArgOrKeyword, ExprCall, PythonVersion};
use ruff_text_size::Ranged;

use crate::{
    FixAvailability, Violation,
    checkers::ast::Checker,
    importer::ImportRequest,
    preview::is_fix_os_stat_enabled,
    rules::flake8_use_pathlib::helpers::{
        has_unknown_keywords_or_starred_expr, is_boolean_literal_or_default,
        is_keyword_only_argument_non_default, is_pathlib_path_call,
    },
};

/// ## What it does
/// Checks for uses of `os.stat`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.stat()` can improve readability over the `os`
/// module's counterparts (e.g., `os.path.stat()`).
///
/// ## Examples
/// ```python
/// import os
/// from pwd import getpwuid
/// from grp import getgrgid
///
/// stat = os.stat(file_name)
/// owner_name = getpwuid(stat.st_uid).pw_name
/// group_name = getgrgid(stat.st_gid).gr_name
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// file_path = Path(file_name)
/// stat = file_path.stat()
/// owner_name = file_path.owner()
/// group_name = file_path.group()
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
/// - [Python documentation: `Path.group`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.group)
/// - [Python documentation: `Path.owner`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.owner)
/// - [Python documentation: `os.stat`](https://docs.python.org/3/library/os.html#os.stat)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct OsStat;

impl Violation for OsStat {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`"
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).stat()`".to_string())
    }
}

// PTH116
pub(crate) fn os_stat(checker: &Checker, call: &ExprCall, segment: &[&str]) {
    if segment != ["os", "stat"] {
        return;
    }

    // `dir_fd` is not supported by pathlib, so check if it's set to non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.stat)
    // ```text
    //           0         1           2
    // os.stat(path, *, dir_fd=None, follow_symlinks=True)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }

    if has_unknown_keywords_or_starred_expr(&call.arguments, &["path", "dir_fd", "follow_symlinks"])
    {
        return;
    }

    let Some(path_args) = call.arguments.find_argument_value("path", 0) else {
        return;
    };

    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(OsStat, call.func.range());

    if !is_fix_os_stat_enabled(checker.settings()) {
        return;
    }

    let method = if checker.target_version() >= PythonVersion::PY310 {
        "stat"
    } else {
        match is_boolean_literal_or_default(&call.arguments, "follow_symlinks") {
            Some(true) => "stat",
            Some(false) => "lstat",
            None => return,
        }
    };

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let locator = checker.locator();
        let path_code = locator.slice(path_args.range());

        let args = |arg: ArgOrKeyword| match arg {
            ArgOrKeyword::Arg(expr) if expr.range() != path_args.range() => {
                Some(locator.slice(expr.range()))
            }
            ArgOrKeyword::Keyword(kw)
                if matches!(kw.arg.as_deref(), Some("follow_symlinks"))
                    && checker.target_version() >= PythonVersion::PY310 =>
            {
                Some(locator.slice(kw.range()))
            }
            _ => None,
        };

        let stat_args = itertools::join(call.arguments.iter_source_order().filter_map(args), ", ");

        let replacement = if is_pathlib_path_call(checker, path_args) {
            format!("{path_code}.{method}({stat_args})")
        } else {
            format!("{binding}({path_code}).{method}({stat_args})")
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
