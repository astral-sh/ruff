use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_makedirs_enabled;
use crate::rules::flake8_use_pathlib::helpers::{has_unknown_keywords, is_pathlib_path_call};
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
    if call.arguments.args.len() > 3 {
        return;
    }
    // We should not offer autofixes if there are keyword arguments
    // that don't match the original function signature
    if has_unknown_keywords(&call.arguments, &["name", "mode", "exist_ok"]) {
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

        let name_code = checker.locator().slice(name.range());

        let mkdir_args = if call.arguments.args.len() == 3 && call.arguments.keywords.is_empty() {
            format!(
                "mode={}, exist_ok={}, parents=True",
                checker.locator().slice(call.arguments.args[1].range()),
                checker.locator().slice(call.arguments.args[2].range()),
            )
        } else {
            call.arguments
                .args
                .iter()
                .skip(1)
                .map(|expr| checker.locator().slice(expr.range()).to_string())
                .chain(call.arguments.keywords.iter().filter_map(|kw| {
                    kw.arg.as_ref().and_then(|arg| {
                        (arg != "name")
                            .then(|| format!("{arg}={}", checker.locator().slice(kw.value.range())))
                    })
                }))
                .chain(std::iter::once("parents=True".to_string()))
                .collect::<Vec<_>>()
                .join(", ")
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
