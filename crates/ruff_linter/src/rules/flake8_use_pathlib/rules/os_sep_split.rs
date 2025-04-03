use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprAttribute};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

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
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than working directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsSepSplit;

impl Violation for OsSepSplit {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Replace `.split(os.sep)` with `Path.parts`".to_string()
    }
}

/// PTH206
pub(crate) fn os_sep_split(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::OS) {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "split" {
        return;
    }

    // Match `.split(os.sep)` or `.split(sep=os.sep)`, but avoid cases in which a `maxsplit` is
    // specified.
    if call.arguments.len() != 1 {
        return;
    }

    let Some(sep) = call.arguments.find_argument_value("sep", 0) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(sep)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "sep"]))
    {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(OsSepSplit, attr.range()));
}
