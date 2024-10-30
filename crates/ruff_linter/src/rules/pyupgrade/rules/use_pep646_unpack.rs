use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprSubscript;

use crate::{checkers::ast::Checker, settings::types::PythonVersion};

/// ## What it does
/// Checks for uses of `Unpack[]` on Python 3.11 and above, and suggests
/// using `*` instead.
///
/// ## Why is this bad?
/// [PEP 646] introduced a new syntax for unpacking sequences based on the `*`
/// operator. This syntax is more concise and readable than the previous
/// `typing.Unpack` syntax.
///
/// ## Example
///
/// ```python
/// from typing import Unpack
///
///
/// def foo(*args: Unpack[tuple[int, ...]]) -> None:
///     pass
/// ```
///
/// Use instead:
///
/// ```python
/// def foo(*args: *tuple[int, ...]) -> None:
///     pass
/// ```
///
/// ## References
/// - [PEP 646](https://peps.python.org/pep-0646/#unpack-for-backwards-compatibility)
#[violation]
pub struct NonPEP646Unpack;

impl Violation for NonPEP646Unpack {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `*` for unpacking")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to `*` for unpacking".to_string())
    }
}

/// UP044
pub(crate) fn use_pep646_unpack(checker: &mut Checker, expr: &ExprSubscript) {
    if checker.settings.target_version < PythonVersion::Py311 {
        return;
    }

    if !checker.semantic().seen_typing() {
        return;
    }

    let ExprSubscript {
        range,
        value,
        slice,
        ..
    } = expr;

    if !checker.semantic().match_typing_expr(value, "Unpack") {
        return;
    }

    let mut diagnostic = Diagnostic::new(NonPEP646Unpack, *range);

    let inner = checker.locator().slice(slice.as_ref());

    if checker.settings.preview.is_enabled() {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            format!("*{inner}"),
            *range,
        )));
    }

    checker.diagnostics.push(diagnostic);
}
