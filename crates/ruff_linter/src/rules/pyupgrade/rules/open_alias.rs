use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `io.open`.
///
/// ## Why is this bad?
/// In Python 3, `io.open` is an alias for `open`. Prefer using `open` directly,
/// as it is more idiomatic.
///
/// ## Example
/// ```python
/// import io
///
/// with io.open("file.txt") as f:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// with open("file.txt") as f:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `io.open`](https://docs.python.org/3/library/io.html#io.open)
#[derive(ViolationMetadata)]
pub(crate) struct OpenAlias;

impl Violation for OpenAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use builtin `open`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with builtin `open`".to_string())
    }
}

/// UP020
pub(crate) fn open_alias(checker: &Checker, expr: &Expr, func: &Expr) {
    if checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["io", "open"]))
    {
        let mut diagnostic = Diagnostic::new(OpenAlias, expr.range());
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                "open",
                expr.start(),
                checker.semantic(),
            )?;
            Ok(Fix::safe_edits(
                Edit::range_replacement(binding, func.range()),
                import_edit,
            ))
        });
        checker.report_diagnostic(diagnostic);
    }
}
