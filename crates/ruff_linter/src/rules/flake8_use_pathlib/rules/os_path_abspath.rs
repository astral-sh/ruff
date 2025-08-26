use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_path_abspath_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_pathlib_path_call,
};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `os.path.abspath`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.resolve()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.abspath()`).
///
/// ## Examples
/// ```python
/// import os
///
/// file_path = os.path.abspath("../path/to/file")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// file_path = Path("../path/to/file").resolve()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## Fix Safety
/// This rule's fix is always marked as unsafe because `Path.resolve()` resolves symlinks, while
/// `os.path.abspath()` does not. If resolving symlinks is important, you may need to use
/// `Path.absolute()`. However, `Path.absolute()` also does not remove any `..` components in a
/// path, unlike `os.path.abspath()` and `Path.resolve()`, so if that specific combination of
/// behaviors is required, there's no existing `pathlib` alternative. See CPython issue
/// [#69200](https://github.com/python/cpython/issues/69200).
///
/// ## References
/// - [Python documentation: `Path.resolve`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.resolve)
/// - [Python documentation: `os.path.abspath`](https://docs.python.org/3/library/os.path.html#os.path.abspath)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathAbspath;

impl Violation for OsPathAbspath {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.abspath()` should be replaced by `Path.resolve()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).resolve()`".to_string())
    }
}

/// PTH100
pub(crate) fn os_path_abspath(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "abspath"] {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    let Some(arg) = call.arguments.find_argument_value("path", 0) else {
        return;
    };

    let arg_code = checker.locator().slice(arg.range());
    let range = call.range();

    let mut diagnostic = checker.report_diagnostic(OsPathAbspath, call.func.range());

    if has_unknown_keywords_or_starred_expr(&call.arguments, &["path"]) {
        return;
    }

    if !is_fix_os_path_abspath_enabled(checker.settings()) {
        return;
    }

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let replacement = if is_pathlib_path_call(checker, arg) {
            format!("{arg_code}.resolve()")
        } else {
            format!("{binding}({arg_code}).resolve()")
        };

        Ok(Fix::unsafe_edits(
            Edit::range_replacement(replacement, range),
            [import_edit],
        ))
    });
}
