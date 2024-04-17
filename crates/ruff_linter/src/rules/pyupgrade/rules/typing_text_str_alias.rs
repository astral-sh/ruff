use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `typing.Text`.
///
/// ## Why is this bad?
/// `typing.Text` is an alias for `str`, and only exists for Python 2
/// compatibility. As of Python 3.11, `typing.Text` is deprecated. Use `str`
/// instead.
///
/// ## Example
/// ```python
/// from typing import Text
///
/// foo: Text = "bar"
/// ```
///
/// Use instead:
/// ```python
/// foo: str = "bar"
/// ```
///
/// ## References
/// - [Python documentation: `typing.Text`](https://docs.python.org/3/library/typing.html#typing.Text)
#[violation]
pub struct TypingTextStrAlias;

impl Violation for TypingTextStrAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`typing.Text` is deprecated, use `str`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `str`".to_string())
    }
}

/// UP019
pub(crate) fn typing_text_str_alias(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::TYPING) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["typing", "Text"]))
    {
        let mut diagnostic = Diagnostic::new(TypingTextStrAlias, expr.range());
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                "str",
                expr.start(),
                checker.semantic(),
            )?;
            Ok(Fix::safe_edits(
                Edit::range_replacement(binding, expr.range()),
                import_edit,
            ))
        });
        checker.diagnostics.push(diagnostic);
    }
}
