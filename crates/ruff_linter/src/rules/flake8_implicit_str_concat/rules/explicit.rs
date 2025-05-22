use anyhow::Result;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, BytesLiteralValue, Expr, ExprStringLiteral, FStringPart, FStringValue, Operator,
    StringLiteralValue,
};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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
#[derive(ViolationMetadata)]
pub(crate) struct ExplicitStringConcatenation;

impl Violation for ExplicitStringConcatenation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Explicitly concatenated string should be implicitly concatenated".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove redundant '+' operator to implicitly concatenate".to_string())
    }
}

/// ISC003
pub(crate) fn explicit(expr: &Expr, checker: &Checker) -> Option<Diagnostic> {
    // If the user sets `allow-multiline` to `false`, then we should allow explicitly concatenated
    // strings that span multiple lines even if this rule is enabled. Otherwise, there's no way
    // for the user to write multiline strings, and that setting is "more explicit" than this rule
    // being enabled.
    if !checker.settings.flake8_implicit_str_concat.allow_multiline {
        return None;
    }

    if let Expr::BinOp(bin_op) = expr {
        let ast::ExprBinOp {
            left,
            right,
            range,
            op,
        } = bin_op;
        if matches!(op, Operator::Add) {
            if matches!(
                left.as_ref(),
                Expr::FString(_) | Expr::StringLiteral(_) | Expr::BytesLiteral(_)
            ) && matches!(
                right.as_ref(),
                Expr::FString(_) | Expr::StringLiteral(_) | Expr::BytesLiteral(_)
            ) && checker.locator().contains_line_break(*range)
            {
                let mut diagnostic = Diagnostic::new(ExplicitStringConcatenation, expr.range());
                diagnostic.try_set_fix(|| generate_fix(checker, bin_op));
                return Some(diagnostic);
            }
        }
    }
    None
}

fn expr_type_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::StringLiteral(_) => "string",
        Expr::BytesLiteral(_) => "bytes",
        Expr::FString(_) => "f-string",
        _ => "unknown",
    }
}

fn generate_fix(checker: &Checker, expr_bin_op: &ast::ExprBinOp) -> Result<Fix> {
    let ast::ExprBinOp {
        left, right, range, ..
    } = expr_bin_op;

    // ByteStrings can only be implicitly concatenated with other ByteStrings
    let replacement = match (left.as_ref(), right.as_ref()) {
        (Expr::StringLiteral(l), Expr::StringLiteral(r)) => {
            let parts = concatenate(l.value.as_slice(), r.value.as_slice());

            Expr::StringLiteral(ast::ExprStringLiteral {
                range: TextRange::default(),
                value: StringLiteralValue::concatenated(parts),
            })
        }

        (Expr::FString(l), Expr::FString(r)) => {
            let parts = concatenate(l.value.as_slice(), r.value.as_slice());
            Expr::FString(ast::ExprFString {
                range: TextRange::default(),
                value: FStringValue::concatenated(parts),
            })
        }

        (Expr::StringLiteral(string), Expr::FString(fstring)) => {
            let parts = concatenate(&string_to_fstring_parts(string), fstring.value.as_slice());
            Expr::FString(ast::ExprFString {
                range: TextRange::default(),
                value: FStringValue::concatenated(parts),
            })
        }

        (Expr::FString(fstring), Expr::StringLiteral(string)) => {
            let parts = concatenate(fstring.value.as_slice(), &string_to_fstring_parts(string));
            Expr::FString(ast::ExprFString {
                range: TextRange::default(),
                value: FStringValue::concatenated(parts),
            })
        }

        (Expr::BytesLiteral(left_bytes), Expr::BytesLiteral(right_bytes)) => {
            let parts = concatenate(left_bytes.value.as_slice(), right_bytes.value.as_slice());
            Expr::BytesLiteral(ast::ExprBytesLiteral {
                range: TextRange::default(),
                value: BytesLiteralValue::concatenated(parts),
            })
        }

        _ => {
            return Err(anyhow::anyhow!(
                "Cannot implicitly concatenate these string types, {} + {}",
                expr_type_name(left.as_ref()),
                expr_type_name(right.as_ref())
            ));
        }
    };

    let content = checker.generator().expr(&replacement);
    Ok(Fix::safe_edit(Edit::range_replacement(content, *range)))
}

fn concatenate<T: Clone>(left: &[T], right: &[T]) -> Vec<T> {
    let mut parts = left.to_vec();
    parts.extend_from_slice(right);
    parts
}

fn string_to_fstring_parts(string: &ExprStringLiteral) -> Vec<FStringPart> {
    string
        .value
        .iter()
        .map(|lit| FStringPart::Literal(lit.clone()))
        .collect()
}
