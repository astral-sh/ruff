use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Operator};
use ruff_python_trivia::is_python_whitespace;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

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
pub(crate) fn explicit(expr: &Expr, checker: &Checker) -> Option<Diagnostic> {
    // If the user sets `allow-multiline` to `false`, then we should allow explicitly concatenated
    // strings that span multiple lines even if this rule is enabled. Otherwise, there's no way
    // for the user to write multiline strings, and that setting is "more explicit" than this rule
    // being enabled.
    if !checker.settings.flake8_implicit_str_concat.allow_multiline {
        return None;
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
                let mut diagnostic = Diagnostic::new(ExplicitStringConcatenation, expr.range());
                diagnostic.set_fix(generate_fix(checker, bin_op));
                return Some(diagnostic);
            }
        }
    }
    None
}

fn generate_fix(checker: &Checker, expr_bin_op: &ast::ExprBinOp) -> Fix {
    let ast::ExprBinOp { left, right, .. } = expr_bin_op;
    let between_operands_range = TextRange::new(left.end(), right.start());
    let between_operands = checker.locator().slice(between_operands_range);
    let plus_pos = between_operands.find('+').unwrap();
    let (before, after) = between_operands.split_at(plus_pos);
    let after = &after[1..]; // Ignore `+` operator

    let linebreak_before_operator = checker.locator().contains_line_break(TextRange::new(
        left.end(),
        left.end() + TextSize::try_from(plus_pos).unwrap(),
    ));

    // If removing `+` from first line trim trailing spaces
    // Preserve indentation when removing `+` from second line
    let before = if linebreak_before_operator {
        before
    } else {
        before.trim_end_matches(is_python_whitespace)
    };

    Fix::safe_edit(Edit::range_replacement(
        format!("{before}{after}"),
        between_operands_range,
    ))
}
