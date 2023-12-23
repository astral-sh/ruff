use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, StringLike};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of string literals that are prefixed with `u`
///
/// ## Why is this bad?
/// String literals prefixed with `u` are redundant in Python >=3.0
///
/// ## Example
/// ```python
/// def foo() -> None:
///     print(u"Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// def foo() -> None:
///     print("Hello, world!")
/// ```
#[violation]
pub struct RedundantUStringPrefix;

impl AlwaysFixableViolation for RedundantUStringPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The `u` prefix is redundant for string literals in Python >=3.0")
    }

    fn fix_title(&self) -> String {
        "Remove `u` prefix".to_string()
    }
}

/// PLW1406
pub(crate) fn redundant_u_string_prefix(checker: &mut Checker, string: StringLike) {
    let StringLike::StringLiteral(ast::ExprStringLiteral { .. }) = string else {
        return;
    };

    let prefix_position = TextRange::new(string.start(), string.start() + TextSize::new(1));

    if checker.locator().slice(prefix_position) != "u" {
        return;
    }

    let mut diagnostic = Diagnostic::new(RedundantUStringPrefix, string.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(prefix_position)));

    checker.diagnostics.push(diagnostic);
}
