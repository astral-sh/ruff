use ruff_python_ast::{self as ast, Expr, Operator};

use crate::settings::LinterSettings;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for string literals that are explicitly concatenated (using the
/// `+` operator).
///
/// ## Why is this bad?
/// For string literals that wrap across multiple lines, implicit string
/// concatenation within parentheses is preferred over explicit
/// concatenation using the `+` operator, as the former is more readable.
///
/// ## Example
/// ```python
/// z = (
///     "The quick brown fox jumps over the lazy "
///     + "dog"
/// )
/// ```
///
/// Use instead:
/// ```python
/// z = (
///     "The quick brown fox jumps over the lazy "
///     "dog"
/// )
/// ```
#[violation]
pub struct ExplicitStringConcatenation;

impl Violation for ExplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Explicitly concatenated string should be implicitly concatenated")
    }
}

/// ISC003
pub(crate) fn explicit(
    expr: &Expr,
    locator: &Locator,
    settings: &LinterSettings,
) -> Option<Diagnostic> {
    // If the user sets `allow-multiline` to `false`, then we should allow explicitly concatenated
    // strings that span multiple lines even if this rule is enabled. Otherwise, there's no way
    // for the user to write multiline strings, and that setting is "more explicit" than this rule
    // being enabled.
    if !settings.flake8_implicit_str_concat.allow_multiline {
        return None;
    }

    if let Expr::BinOp(ast::ExprBinOp {
        left,
        op,
        right,
        range,
    }) = expr
    {
        if matches!(op, Operator::Add) {
            if matches!(
                left.as_ref(),
                Expr::FString(_) | Expr::StringLiteral(_) | Expr::BytesLiteral(_)
            ) && matches!(
                right.as_ref(),
                Expr::FString(_) | Expr::StringLiteral(_) | Expr::BytesLiteral(_)
            ) && locator.contains_line_break(*range)
            {
                return Some(Diagnostic::new(ExplicitStringConcatenation, expr.range()));
            }
        }
    }
    None
}
