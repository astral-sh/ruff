use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ArgOrKeyword, Expr, ExprBooleanLiteral, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_builtin_open_enabled;
use crate::rules::flake8_use_pathlib::helpers::{
    has_unknown_keywords_or_starred_expr, is_argument_non_default, is_file_descriptor,
    is_pathlib_path_call,
};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of the `open()` builtin.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation. When possible,
/// using `Path` object methods such as `Path.open()` can improve readability
/// over the `open` builtin.
///
/// ## Examples
/// ```python
/// with open("f1.py", "wb") as fp:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// with Path("f1.py").open("wb") as fp:
///     ...
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than working directly with strings,
/// especially on older versions of Python.
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the original expression.
///
/// ## References
/// - [Python documentation: `Path.open`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.open)
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#corresponding-tools)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231")]
pub(crate) struct BuiltinOpen;

impl Violation for BuiltinOpen {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`open()` should be replaced by `Path.open()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path.open()`".to_string())
    }
}

// PTH123
pub(crate) fn builtin_open(checker: &Checker, call: &ExprCall, segments: &[&str]) {
    // `closefd` and `opener` are not supported by pathlib, so check if they
    // are set to non-default values.
    // https://github.com/astral-sh/ruff/issues/7620
    // Signature as of Python 3.11 (https://docs.python.org/3/library/functions.html#open):
    // ```text
    // builtins.open(
    //   file,          0
    //   mode='r',      1
    //   buffering=-1,  2
    //   encoding=None, 3
    //   errors=None,   4
    //   newline=None,  5
    //   closefd=True,  6 <= not supported
    //   opener=None    7 <= not supported
    // )
    // ```
    // For `pathlib` (https://docs.python.org/3/library/pathlib.html#pathlib.Path.open):
    // ```text
    // Path.open(mode='r', buffering=-1, encoding=None, errors=None, newline=None)
    // ```
    let file_arg = call.arguments.find_argument_value("file", 0);

    if call
        .arguments
        .find_argument_value("closefd", 6)
        .is_some_and(|expr| {
            !matches!(
                expr,
                Expr::BooleanLiteral(ExprBooleanLiteral { value: true, .. })
            )
        })
        || is_argument_non_default(&call.arguments, "opener", 7)
        || file_arg.is_some_and(|expr| is_file_descriptor(expr, checker.semantic()))
    {
        return;
    }

    if !matches!(segments, ["" | "builtins", "open"]) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(BuiltinOpen, call.func.range());

    if !is_fix_builtin_open_enabled(checker.settings()) {
        return;
    }

    let Some(file) = file_arg else {
        return;
    };

    if has_unknown_keywords_or_starred_expr(
        &call.arguments,
        &[
            "file",
            "mode",
            "buffering",
            "encoding",
            "errors",
            "newline",
            "closefd",
            "opener",
        ],
    ) {
        return;
    }

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let locator = checker.locator();
        let file_code = locator.slice(file.range());

        let args = |i: usize, arg: ArgOrKeyword| match arg {
            ArgOrKeyword::Arg(expr) => {
                if expr.range() == file.range() || i == 6 || i == 7 {
                    None
                } else {
                    Some(locator.slice(expr.range()))
                }
            }
            ArgOrKeyword::Keyword(kw) => match kw.arg.as_deref() {
                Some("mode" | "buffering" | "encoding" | "errors" | "newline") => {
                    Some(locator.slice(kw))
                }
                _ => None,
            },
        };

        let open_args = itertools::join(
            call.arguments
                .arguments_source_order()
                .enumerate()
                .filter_map(|(i, arg)| args(i, arg)),
            ", ",
        );

        let replacement = if is_pathlib_path_call(checker, file) {
            format!("{file_code}.open({open_args})")
        } else {
            format!("{binding}({file_code}).open({open_args})")
        };

        let range = call.range();

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
