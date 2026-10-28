use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::importer::ImportRequest;
use crate::rules::flake8_use_pathlib::helpers::is_keyword_only_argument_non_default;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for the use of `glob.glob()` and `glob.iglob()`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os` and `glob`.
///
/// When possible, using `Path` object methods such as `Path.glob()` can
/// improve readability over their low-level counterparts (e.g.,
/// `glob.glob()`).
///
/// Note that `glob.glob()` and `Path.glob()` are not exact equivalents:
///
/// |                   | `glob`-module functions                                                                                                                              | `Path.glob()`                                                                                                                                |
/// |-------------------|------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------|
/// | Hidden files      | Hidden files are excluded by default. On Python 3.11+, the `include_hidden` keyword can be used to include hidden directories.                       | Includes hidden files by default.                                                                                                            |
/// | Eagerness         | `glob.iglob()` returns a lazy iterator. Under the hood, `glob.glob()` simply converts the iterator to a list.                                        | `Path.glob()` returns a lazy iterator.                                                                                                       |
/// | Working directory | `glob.glob()` and `glob.iglob()` take a `root_dir` keyword to set the current working directory.                                                     | `Path.rglob()` can be used to return the relative path.                                                                                      |
/// | Globstar (`**`)   | The `recursive` flag must be set to `True` for the `**` pattern to match any files and zero or more directories, subdirectories, and symbolic links. | The `**` pattern in `Path.glob()` means "this directory and all subdirectories, recursively". In other words, it enables recursive globbing. |
///
/// ## Example
/// ```python
/// import glob
/// import os
///
/// glob.glob(os.path.join("my_path", "requirements*.txt"))
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("my_path").glob("requirements*.txt")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## Fix safety
/// In preview, a display-only fix is available for literal relative patterns.
/// The suggestion requires manual review: `Path.glob()` yields `Path` objects
/// instead of strings, includes hidden files, and may differ in its handling of
/// symlinks. Unlike `glob.glob()`, it does not return a list.
///
/// ## References
/// - [Python documentation: `Path.glob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.glob)
/// - [Python documentation: `Path.rglob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rglob)
/// - [Python documentation: `glob.glob`](https://docs.python.org/3/library/glob.html#glob.glob)
/// - [Python documentation: `glob.iglob`](https://docs.python.org/3/library/glob.html#glob.iglob)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Pedantic)]
pub(crate) struct Glob {
    pub function: String,
}

impl Violation for Glob {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Glob { function } = self;
        format!("Replace `{function}` with `Path.glob` or `Path.rglob`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).glob(...)`".to_string())
    }
}

pub(crate) fn glob_call(checker: &Checker, call: &ExprCall, function: &str) {
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }
    let Some(mut diagnostic) = checker.report_diagnostic_if_enabled(
        Glob {
            function: function.to_string(),
        },
        call.func.range(),
    ) else {
        return;
    };
    if !checker.settings().preview.is_enabled()
        || call.arguments.args.len() > 1
        || checker.comment_ranges().intersects(call.range())
        || call.arguments.keywords.iter().any(|keyword| {
            !matches!(
                keyword.arg.as_deref(),
                Some("pathname" | "root_dir" | "dir_fd" | "recursive" | "include_hidden")
            )
        })
    {
        return;
    }
    let Some(Expr::StringLiteral(pattern)) = call.arguments.find_argument_value("pathname", 0)
    else {
        return;
    };
    let value = pattern.value.to_str();
    // Path.glob requires a relative pattern. Avoid platform-dependent drive
    // prefixes and separators rather than interpreting them on the host OS.
    if value.is_empty() || value.starts_with('/') || value.contains(['\\', ':']) {
        return;
    }
    let recursive = match call.arguments.find_keyword("recursive") {
        None => false,
        Some(keyword) => match &keyword.value {
            Expr::BooleanLiteral(boolean) => boolean.value,
            _ => return,
        },
    };
    if value.contains("**")
        && (!recursive
            || value
                .split('/')
                .any(|part| part.contains("**") && part != "**"))
    {
        return;
    }
    if call
        .arguments
        .find_keyword("include_hidden")
        .is_some_and(|keyword| !matches!(&keyword.value, Expr::BooleanLiteral(_)))
    {
        return;
    }
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;
        let root = call
            .arguments
            .find_keyword("root_dir")
            .filter(|keyword| !keyword.value.is_none_literal_expr())
            .map_or("", |keyword| checker.locator().slice(&keyword.value));
        Ok(Fix::applicable_edits(
            Edit::range_replacement(
                format!(
                    "{binding}({root}).glob({})",
                    checker.locator().slice(pattern)
                ),
                call.range(),
            ),
            [import_edit],
            Applicability::DisplayOnly,
        ))
    });
}
