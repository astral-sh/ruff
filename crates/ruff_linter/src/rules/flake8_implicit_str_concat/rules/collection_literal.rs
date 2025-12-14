use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{Expr, StringLike};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for implicitly concatenated strings inside list, tuple, and set literals.
///
/// ## Why is this bad?
/// In collection literals, implicit string concatenation is often the result of
/// a missing comma between elements, which can silently merge items together.
///
/// ## Example
/// ```python
/// facts = (
///     "Lobsters have blue blood.",
///     "The liver is the only human organ that can fully regenerate itself.",
///     "Clarinets are made almost entirely out of wood from the mpingo tree."
///     "In 1971, astronaut Alan Shepard played golf on the moon.",
/// )
/// ```
///
/// Instead, you likely intended:
/// ```python
/// facts = (
///     "Lobsters have blue blood.",
///     "The liver is the only human organ that can fully regenerate itself.",
///     "Clarinets are made almost entirely out of wood from the mpingo tree.",
///     "In 1971, astronaut Alan Shepard played golf on the moon.",
/// )
/// ```
///
/// If the concatenation is intentional, wrap it in parentheses to make it
/// explicit:
/// ```python
/// facts = (
///     "Lobsters have blue blood.",
///     "The liver is the only human organ that can fully regenerate itself.",
///     (
///         "Clarinets are made almost entirely out of wood from the mpingo tree."
///         "In 1971, astronaut Alan Shepard played golf on the moon."
///     ),
/// )
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.9")]
pub(crate) struct ImplicitStringConcatenationInCollectionLiteral;

impl Violation for ImplicitStringConcatenationInCollectionLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Implicit string concatenation in collection literal; did you forget a comma?".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Wrap implicitly concatenated strings in parentheses".to_string())
    }
}

/// ISC004
pub(crate) fn implicit_string_concatenation_in_collection_literal(
    checker: &Checker,
    expr: &Expr,
    elements: &[Expr],
) {
    for element in elements {
        let Ok(string_like) = StringLike::try_from(element) else {
            continue;
        };
        if !string_like.is_implicit_concatenated() {
            continue;
        }
        if parenthesized_range(
            string_like.as_expression_ref(),
            expr.into(),
            checker.tokens(),
        )
        .is_some()
        {
            continue;
        }

        let mut diagnostic = checker.report_diagnostic(
            ImplicitStringConcatenationInCollectionLiteral,
            string_like.range(),
        );
        diagnostic.set_fix(Fix::safe_edits(
            Edit::insertion("(".to_string(), string_like.range().start()),
            [Edit::insertion(")".to_string(), string_like.range().end())],
        ));
    }
}
