use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;
use ruff_python_ast::{Expr, ExprAttribute, Keyword, Ranged};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `.split(os.sep)`
///
/// ## Why is this bad?
/// The `pathlib` module in the standard library should be used for path
/// manipulation. It provides a high-level API with the functionality
/// needed for common operations on `Path` objects.
///
/// ## Example
/// If not all parts of the path are needed, then the `name` and `parent`
/// attributes of the `Path` object should be used. Otherwise, the `parts`
/// attribute can be used as shown in the last example.
/// ```python
/// import os
///
/// "path/to/file_name.txt".split(os.sep)[-1]
///
/// "path/to/file_name.txt".split(os.sep)[-2]
///
/// # Iterating over the path parts
/// if any(part in blocklist for part in "my/file/path".split(os.sep)):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("path/to/file_name.txt").name
///
/// Path("path/to/file_name.txt").parent.name
///
/// # Iterating over the path parts
/// if any(part in blocklist for part in Path("my/file/path").parts):
///     ...
/// ```
#[violation]
pub struct OsSepSplit;

impl Violation for OsSepSplit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace `.split(os.sep)` with `Path.parts`")
    }
}

/// PTH206
pub(crate) fn os_sep_split(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Expr::Attribute(ExprAttribute { attr, .. }) = func else {
        return;
    };

    if attr.as_str() != "split" {
        return;
    };

    let sep = if !args.is_empty() {
        // `.split(os.sep)`
        let [arg] = args else {
            return;
        };
        arg
    } else if !keywords.is_empty() {
        // `.split(sep=os.sep)`
        let Some(keyword) = find_keyword(keywords, "sep") else {
            return;
        };
        &keyword.value
    } else {
        return;
    };

    if !checker
        .semantic()
        .resolve_call_path(sep)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["os", "sep"])
        })
    {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(OsSepSplit, attr.range()));
}
