use std::borrow::Cow;

use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableStmt;
use ruff_python_ast::stmt_if::{IfElifBranch, if_elif_branches};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `if` branches with identical arm bodies.
///
/// ## Why is this bad?
/// If multiple arms of an `if` statement have the same body, using `or`
/// better signals the intent of the statement.
///
/// ## Example
/// ```python
/// if x == 1:
///     print("Hello")
/// elif x == 2:
///     print("Hello")
/// ```
///
/// Use instead:
/// ```python
/// if x == 1 or x == 2:
///     print("Hello")
/// ```
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe if the `if`/`elif` branches contain
/// comments that would be deleted by merging the branches (e.g., comments
/// between the branches, inline comments on the condition, or differing
/// comments in the bodies). Otherwise, the fix can be applied safely.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.246")]
pub(crate) struct IfWithSameArms;

impl Violation for IfWithSameArms {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Combine `if` branches using logical `or` operator".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Combine `if` branches".to_string())
    }
}

/// SIM114
pub(crate) fn if_with_same_arms(checker: &Checker, stmt_if: &ast::StmtIf) {
    let mut branches_iter = if_elif_branches(stmt_if).peekable();
    while let Some(current_branch) = branches_iter.next() {
        let Some(following_branch) = branches_iter.peek() else {
            continue;
        };

        // The bodies must have the same code ...
        if current_branch.body.len() != following_branch.body.len() {
            continue;
        }
        if !current_branch
            .body
            .iter()
            .zip(following_branch.body)
            .all(|(stmt1, stmt2)| ComparableStmt::from(stmt1) == ComparableStmt::from(stmt2))
        {
            continue;
        }

        // Check if body comments differ — if so, the fix removes comments
        let first_comments = checker
            .comment_ranges()
            .comments_in_range(body_range(&current_branch, checker.locator()))
            .iter()
            .map(|range| checker.locator().slice(*range));
        let second_comments = checker
            .comment_ranges()
            .comments_in_range(body_range(following_branch, checker.locator()))
            .iter()
            .map(|range| checker.locator().slice(*range));
        let has_different_body_comments = !first_comments.eq(second_comments);

        // Check for comments between the branches or on the following branch's
        // test line (e.g., inline comments in conditions, trailing comments on
        // the elif line). These would be lost when the fix merges the branches.
        let has_non_body_comments = !checker
            .comment_ranges()
            .comments_in_range(TextRange::new(
                checker.locator().full_line_end(current_branch.end()),
                checker.locator().line_end(following_branch.test.end()),
            ))
            .is_empty();

        let applicability = if has_different_body_comments || has_non_body_comments {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

        let mut diagnostic = checker.report_diagnostic(
            IfWithSameArms,
            TextRange::new(current_branch.start(), following_branch.end()),
        );

        diagnostic.try_set_fix(|| {
            merge_branches(
                stmt_if,
                &current_branch,
                following_branch,
                checker.locator(),
                checker.tokens(),
                applicability,
            )
        });
    }
}

/// Generate a [`Fix`] to merge two [`IfElifBranch`] branches.
fn merge_branches(
    stmt_if: &ast::StmtIf,
    current_branch: &IfElifBranch,
    following_branch: &IfElifBranch,
    locator: &Locator,
    tokens: &ruff_python_ast::token::Tokens,
    applicability: Applicability,
) -> Result<Fix> {
    // Identify the colon (`:`) at the end of the current branch's test.
    let Some(current_branch_colon) =
        SimpleTokenizer::starts_at(current_branch.test.end(), locator.contents())
            .find(|token| token.kind == SimpleTokenKind::Colon)
    else {
        return Err(anyhow::anyhow!("Expected colon after test"));
    };

    let deletion_edit = Edit::deletion(
        locator.full_line_end(current_branch.end()),
        locator.full_line_end(following_branch.end()),
    );

    // If the following test isn't parenthesized, consider parenthesizing it.
    let following_branch_test = if let Some(range) =
        parenthesized_range(following_branch.test.into(), stmt_if.into(), tokens)
    {
        Cow::Borrowed(locator.slice(range))
    } else if matches!(
        following_branch.test,
        Expr::Lambda(_) | Expr::Named(_) | Expr::If(_)
    ) {
        // If the following expressions binds more tightly than `or`, parenthesize it.
        Cow::Owned(format!("({})", locator.slice(following_branch.test)))
    } else {
        Cow::Borrowed(locator.slice(following_branch.test))
    };

    let insertion_edit = Edit::insertion(
        format!(" or {following_branch_test}"),
        current_branch_colon.start(),
    );

    // If the current test isn't parenthesized, consider parenthesizing it.
    //
    // For example, if the current test is `x if x else y`, we should parenthesize it to
    // `(x if x else y) or ...`.
    let parenthesize_edit =
        if matches!(
            current_branch.test,
            Expr::Lambda(_) | Expr::Named(_) | Expr::If(_)
        ) && parenthesized_range(current_branch.test.into(), stmt_if.into(), tokens).is_none()
        {
            Some(Edit::range_replacement(
                format!("({})", locator.slice(current_branch.test)),
                current_branch.test.range(),
            ))
        } else {
            None
        };

    Ok(Fix::applicable_edits(
        deletion_edit,
        parenthesize_edit.into_iter().chain(Some(insertion_edit)),
        applicability,
    ))
}

/// Return the [`TextRange`] of an [`IfElifBranch`]'s body (from the end of the test to the end of
/// the body).
fn body_range(branch: &IfElifBranch, locator: &Locator) -> TextRange {
    TextRange::new(
        locator.line_end(branch.test.end()),
        locator.line_end(branch.end()),
    )
}
