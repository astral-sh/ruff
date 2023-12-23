use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, StringLike};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of explicit Unicode prefixes on string literals.
///
/// ## Why is this bad?
/// In Python 3, all strings are Unicode by default, making the `u` prefix
/// redundant.
///
/// ## Example
/// ```python
/// def func() -> None:
///     print(u"Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// def func() -> None:
///     print("Hello, world!")
/// ```
#[violation]
pub struct RedundantUnicodePrefix;

impl AlwaysFixableViolation for RedundantUnicodePrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The `u` prefix is redundant for string literals in Python 3")
    }

    fn fix_title(&self) -> String {
        "Remove `u` prefix".to_string()
    }
}

/// PLW1406
pub(crate) fn redundant_unicode_prefix(checker: &mut Checker, string: StringLike) {
    let StringLike::StringLiteral(ast::ExprStringLiteral { .. }) = string else {
        return;
    };

    // If a string has a Unicode prefix, it must be exactly `u` or `U`.
    let prefix_range = TextRange::new(string.start(), string.start() + TextSize::new(1));
    let prefix_text = checker.locator().slice(prefix_range);
    if prefix_text != "u" && prefix_text != "U" {
        return;
    }

    let mut diagnostic = Diagnostic::new(RedundantUnicodePrefix, string.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(prefix_range)));
    checker.diagnostics.push(diagnostic);
}
