use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprNumberLiteral;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Example
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct BadNumericLiteralFormat {
    source: String,
    replacement: String,
}

impl AlwaysFixableViolation for BadNumericLiteralFormat {
    /// Implements the user-readable message for the violation.
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The numeric literal `{}` has a bad format. Consider replacing it with `{}`.",
            self.source, self.replacement
        )
    }

    /// Implements a title for the fix action.
    fn fix_title(&self) -> String {
        "Replace with properly formatted numeric literal".to_string()
    }
}

/// WPS987
pub(crate) fn bad_numeric_literal_format(checker: &mut Checker, number_literal: &ExprNumberLiteral) {
    let ExprNumberLiteral {
        range,
        ..
    } = number_literal;
    let text = &checker.locator().contents()[number_literal.range()];
    let mut normalized = text.to_lowercase();

    if normalized.starts_with("0o") || normalized.starts_with("0b") {
        // Leave octal and binary literals alone.
    } else if normalized.starts_with("0x") {
        normalized = format_hex(&normalized);
    } else if normalized.contains('e') {
        normalized = format_scientific_notation(&normalized);
    } else if normalized.ends_with('j') {
        normalized = format_complex_number(&normalized);
    } else {
        normalized = format_float_or_int_string(&normalized);
    }

    if normalized != text {
        return;
    }
    checker.diagnostics.push(
        Diagnostic::new(
            BadNumericLiteralFormat {
                source: text.to_string(),
                replacement: normalized.clone(),
            },
            *range,
        )
        .with_fix(Fix::safe_edit(Edit::range_replacement(
            normalized,
            *range
        ))),
    );
}


fn format_hex(text: &str) -> String {
    let (before, after) = text.split_at(2); // Split at the "0x" prefix.
    format!("{}{}", before, after.to_uppercase())
}

/// Formats a numeric string utilizing scientific notation.
fn format_scientific_notation(text: &str) -> String {
    if let Some((before, after)) = text.split_once('e') {
        let (sign, exponent) = if after.starts_with('-') {
            ("-", after.strip_prefix('-'))
        } else if after.starts_with('+') {
            ("", after.strip_prefix('+'))
        } else {
            ("", Some(after))
        };

        format!(
            "{}e{}{}",
            format_float_or_int_string(before),
            sign,
            exponent.unwrap_or(after)
        )
    } else {
        text.to_string() // Fallback, though this shouldn't happen.
    }
}

/// Formats a complex number string like "10j".
fn format_complex_number(text: &str) -> String {
    let number = &text[..text.len() - 1]; // All but the last character.
    let suffix = &text[text.len() - 1..]; // The last character.
    format!("{}{}", format_float_or_int_string(number), suffix)
}

/// Formats a float or integer string like "1.0".
fn format_float_or_int_string(text: &str) -> String {
    if let Some((before, after)) = text.split_once('.') {
        let before = if before.is_empty() { "0" } else { before };
        let after = if after.is_empty() { "0" } else { after };
        format!("{before}.{after}")
    } else {
        text.to_string()
    }
}