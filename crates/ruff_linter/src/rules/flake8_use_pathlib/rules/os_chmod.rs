use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ArgOrKeyword, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_chmod_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_file_descriptor, is_keyword_only_argument_non_default,
    is_optional_bool_literal, is_pathlib_path_call,
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

    // Needs a minimum two arguments for fix
    if call.arguments.len() < 2 {
        return;
    }

    if !is_fix_os_chmod_enabled(checker.settings()) {
        return;
    }

    if has_unknown_keywords_or_starred_expr(
        &call.arguments,
        &["path", "mode", "dir_fd", "follow_symlinks"],
    ) {
        return;
    }

    let Some(path) = path_arg else {
        return;
    };

    if !is_optional_bool_literal(&call.arguments, "follow_symlinks", 3) {
        return;
    }

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let locator = checker.locator();
        let path_code = locator.slice(path.range());

        let (mode, follow_symlinks) = (
            call.arguments.find_argument("mode", 1),
            call.arguments.find_argument("follow_symlinks", 3),
        );

        let args = |arg: &ArgOrKeyword| match arg {
            ArgOrKeyword::Arg(expr) => locator.slice(expr),
            ArgOrKeyword::Keyword(keyword) => locator.slice(&keyword.value),
        };

        let follow_symlinks_is_false = |arg: &ArgOrKeyword| {
            let expr = match arg {
                ArgOrKeyword::Arg(e) => e,
                ArgOrKeyword::Keyword(k) => &k.value,
            };
            expr.as_boolean_literal_expr().is_some_and(|bl| !bl.value)
        };

        let chmod_args = match (mode, follow_symlinks) {
            (Some(m), Some(f)) if follow_symlinks_is_false(&f) => {
                format!("{}, follow_symlinks=False", args(&m))
            }
            (Some(arg), _) | (_, Some(arg)) => args(&arg).to_string(),
            _ => String::new(),
        };

        let replacement = if is_pathlib_path_call(checker, path) {
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
