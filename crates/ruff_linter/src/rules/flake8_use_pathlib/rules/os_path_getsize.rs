use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::preview::is_fix_os_path_getsize_enabled;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `os.path.getsize`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`.
///
/// When possible, using `Path` object methods such as `Path.stat()` can
/// improve readability over the `os.path` module's counterparts (e.g.,
/// `os.path.getsize()`).
///
/// ## Example
/// ```python
/// import os
///
/// os.path.getsize(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).stat().st_size
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
/// - [Python documentation: `os.path.getsize`](https://docs.python.org/3/library/os.path.html#os.path.getsize)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathGetsize;

impl Violation for OsPathGetsize {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.getsize` should be replaced by `Path.stat().st_size`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Path(...).stat().st_size`".to_string())
    }
}

/// PTH202
pub(crate) fn os_path_getsize(checker: &Checker, call: &ExprCall) {
    if !matches!(
        checker
            .semantic()
            .resolve_qualified_name(&call.func)
            .as_ref()
            .map(QualifiedName::segments),
        Some(["os", "path", "getsize"])
    ) {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    let Some(arg) = call.arguments.find_argument_value("filename", 0) else {
        return;
    };

    let arg_code = checker.locator().slice(arg.range());
    let range = call.range();

    let applicability = if checker.comment_ranges().intersects(range) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let mut diagnostic = checker.report_diagnostic(OsPathGetsize, range);

    if is_fix_os_path_getsize_enabled(checker.settings()) {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("pathlib", "Path"),
                call.start(),
                checker.semantic(),
            )?;

            let replacement = if is_path_call(checker, arg) {
                format!("{arg_code}.stat().st_size")
            } else {
                format!("{binding}({arg_code}).stat().st_size")
            };

            Ok(
                Fix::safe_edits(Edit::range_replacement(replacement, range), [import_edit])
                    .with_applicability(applicability),
            )
        });
    }
}

fn is_path_call(checker: &Checker, expr: &Expr) -> bool {
    expr.as_call_expr().is_some_and(|expr_call| {
        checker
            .semantic()
            .resolve_qualified_name(&expr_call.func)
            .is_some_and(|name| matches!(name.segments(), ["pathlib", "Path"]))
    })
}
