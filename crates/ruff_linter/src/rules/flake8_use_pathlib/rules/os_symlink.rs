use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_symlink_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_keyword_only_argument_non_default,
    is_pathlib_path_call,
};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.symlink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.symlink`.
///
/// ## Example
/// ```python
/// import os
///
/// os.symlink("usr/bin/python", "tmp/python", target_is_directory=False)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("tmp/python").symlink_to("usr/bin/python")
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
/// - [Python documentation: `Path.symlink_to`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.symlink_to)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.13.0")]
pub(crate) struct OsSymlink;

impl Violation for OsSymlink {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.symlink` should be replaced by `Path.symlink_to`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).symlink_to(...)`".to_string())
    }
}

/// PTH211
pub(crate) fn os_symlink(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "symlink"] {
        return;
    }

    // `dir_fd` is not supported by pathlib, so check if there are non-default values.
    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.symlink)
    // ```text
    //            0    1    2                             3
    // os.symlink(src, dst, target_is_directory=False, *, dir_fd=None)
    // ```
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }

    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(OsSymlink, call.func.range());

    if !is_fix_os_symlink_enabled(checker.settings()) {
        return;
    }

    if call.arguments.len() > 3 {
        return;
    }

    if has_unknown_keywords_or_starred_expr(
        &call.arguments,
        &["src", "dst", "target_is_directory", "dir_fd"],
    ) {
        return;
    }

    let (Some(src), Some(dst)) = (
        call.arguments.find_argument_value("src", 0),
        call.arguments.find_argument_value("dst", 1),
    ) else {
        return;
    };

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

        let locator = checker.locator();
        let src_code = locator.slice(src.range());
        let dst_code = locator.slice(dst.range());

        let target_is_directory = call
            .arguments
            .find_argument_value("target_is_directory", 2)
            .and_then(|expr| {
                let code = locator.slice(expr.range());
                expr.as_boolean_literal_expr()
                    .is_none_or(|bl| bl.value)
                    .then_some(format!(", target_is_directory={code}"))
            })
            .unwrap_or_default();

        let replacement = if is_pathlib_path_call(checker, dst) {
            format!("{dst_code}.symlink_to({src_code}{target_is_directory})")
        } else {
            format!("{binding}({dst_code}).symlink_to({src_code}{target_is_directory})")
        };

        Ok(Fix::applicable_edits(
            Edit::range_replacement(replacement, range),
            [import_edit],
            applicability,
        ))
    });
}
