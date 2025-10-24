use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ArgOrKeyword, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_makedirs_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_pathlib_path_call,
};
use crate::{FixAvailability, Violation};

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
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
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

    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(OsMakedirs, call.func.range());

    let Some(name) = call.arguments.find_argument_value("name", 0) else {
        return;
    };

    if !is_fix_os_makedirs_enabled(checker.settings()) {
        return;
    }

    // Signature as of Python 3.13 (https://docs.python.org/3/library/os.html#os.makedirs)
    // ```text
    //               0      1            2
    //  os.makedirs(name, mode=0o777, exist_ok=False)
    // ```
    // We should not offer autofixes if there are more arguments
    // than in the original signature
    if call.arguments.len() > 3 {
        return;
    }
    // We should not offer autofixes if there are keyword arguments
    // that don't match the original function signature
    if has_unknown_keywords_or_starred_expr(&call.arguments, &["name", "mode", "exist_ok"]) {
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

        let locator = checker.locator();

        let name_code = locator.slice(name.range());

        let mode = call.arguments.find_argument("mode", 1);
        let exist_ok = call.arguments.find_argument("exist_ok", 2);

        let mkdir_args = match (mode, exist_ok) {
            // Default to a keyword argument when alone.
            (None, None) => "parents=True".to_string(),
            // If either argument is missing, it's safe to add `parents` at the end.
            (None, Some(arg)) | (Some(arg), None) => {
                format!("{}, parents=True", locator.slice(arg))
            }
            // If they're all positional, `parents` has to be positional too.
            (Some(ArgOrKeyword::Arg(mode)), Some(ArgOrKeyword::Arg(exist_ok))) => {
                format!("{}, True, {}", locator.slice(mode), locator.slice(exist_ok))
            }
            // If either argument is a keyword, we can put `parents` at the end again.
            (Some(mode), Some(exist_ok)) => format!(
                "{}, {}, parents=True",
                locator.slice(mode),
                locator.slice(exist_ok)
            ),
        };

        let replacement = if is_pathlib_path_call(checker, name) {
            format!("{name_code}.mkdir({mkdir_args})")
        } else {
            format!("{binding}({name_code}).mkdir({mkdir_args})")
        };

        Ok(Fix::applicable_edits(
            Edit::range_replacement(replacement, range),
            [import_edit],
            applicability,
        ))
    });
}
