use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_python_trivia::is_python_whitespace;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::AlwaysFixableViolation;
use crate::checkers::ast::Checker;
use crate::{Edit, Fix};

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

impl AlwaysFixableViolation for ExplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Explicitly concatenated string should be implicitly concatenated".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove redundant '+' operator to implicitly concatenate".to_string()
    }
}

/// ISC003
pub(crate) fn explicit(checker: &Checker, expr: &Expr) {
    // If the user sets `allow-multiline` to `false`, then we should allow explicitly concatenated
    // strings that span multiple lines even if this rule is enabled. Otherwise, there's no way
    // for the user to write multiline strings, and that setting is "more explicit" than this rule
    // being enabled.
    if !checker
        .settings()
        .flake8_implicit_str_concat
        .allow_multiline
    {
        return;
    }

    if let Expr::BinOp(bin_op) = expr {
        if let ast::ExprBinOp {
            left,
            right,
            op: Operator::Add,
            ..
        } = bin_op
        {
            let concatable = matches!(
                (left.as_ref(), right.as_ref()),
                (
                    Expr::StringLiteral(_) | Expr::FString(_),
                    Expr::StringLiteral(_) | Expr::FString(_)
                ) | (Expr::BytesLiteral(_), Expr::BytesLiteral(_))
            );
            if concatable
                && checker
                    .locator()
                    .contains_line_break(TextRange::new(left.end(), right.start()))
            {
                checker
                    .report_diagnostic(ExplicitStringConcatenation, expr.range())
                    .set_fix(generate_fix(checker, bin_op));
            }
        }
    }
}

fn generate_fix(checker: &Checker, expr_bin_op: &ast::ExprBinOp) -> Fix {
    let ast::ExprBinOp { left, right, .. } = expr_bin_op;

    let between_operands_range = TextRange::new(left.end(), right.start());
    let between_operands = checker.locator().slice(between_operands_range);
    let (before_plus, after_plus) = between_operands.split_once('+').unwrap();

    let linebreak_before_operator =
        before_plus.contains_line_break(TextRange::at(TextSize::new(0), before_plus.text_len()));

    // If removing `+` from first line trim trailing spaces
    // Preserve indentation when removing `+` from second line
    let before_plus = if linebreak_before_operator {
        before_plus
    } else {
        before_plus.trim_end_matches(is_python_whitespace)
    };

    Fix::safe_edit(Edit::range_replacement(
        format!("{before_plus}{after_plus}"),
        between_operands_range,
    ))
}
