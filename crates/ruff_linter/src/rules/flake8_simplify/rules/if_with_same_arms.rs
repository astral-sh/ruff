use std::borrow::Cow;

use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableStmt;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::stmt_if::{IfElifBranch, if_elif_branches};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::{CommentRanges, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

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
#[derive(ViolationMetadata)]
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

        // The bodies must have the same code
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

        // Use the new function to check if comments can be safely preserved
        if !can_preserve_comments_during_merge(
            &current_branch,
            following_branch,
            checker.locator(),
            checker.comment_ranges(),
        ) {
            // Still report the diagnostic but without a fix
            checker.report_diagnostic(
                IfWithSameArms,
                TextRange::new(current_branch.start(), following_branch.end()),
            );
            continue;
        }

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
                checker.comment_ranges(),
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
    comment_ranges: &CommentRanges,
) -> Result<Fix> {
    // Identify the colon (`:`) at the end of the current branch's test.
    let Some(current_branch_colon) =
        SimpleTokenizer::starts_at(current_branch.test.end(), locator.contents())
            .find(|token| token.kind == SimpleTokenKind::Colon)
    else {
        return Err(anyhow::anyhow!("Expected colon after test"));
    };

    // Since can_preserve_comments_during_merge already verified it's safe,
    // we can proceed with the merge
    let deletion_edit = Edit::deletion(
        locator.full_line_end(current_branch.end()),
        locator.full_line_end(following_branch.end()),
    );

    // Rest of your existing code for handling parentheses and insertion...
    let following_branch_test = if let Some(range) = parenthesized_range(
        following_branch.test.into(),
        stmt_if.into(),
        comment_ranges,
        locator.contents(),
    ) {
        Cow::Borrowed(locator.slice(range))
    } else if matches!(
        following_branch.test,
        Expr::Lambda(_) | Expr::Named(_) | Expr::If(_)
    ) {
        Cow::Owned(format!("({})", locator.slice(following_branch.test)))
    } else {
        Cow::Borrowed(locator.slice(following_branch.test))
    };

    let insertion_edit = Edit::insertion(
        format!(" or {following_branch_test}"),
        current_branch_colon.start(),
    );

    let parenthesize_edit = if matches!(
        current_branch.test,
        Expr::Lambda(_) | Expr::Named(_) | Expr::If(_)
    ) && parenthesized_range(
        current_branch.test.into(),
        stmt_if.into(),
        comment_ranges,
        locator.contents(),
    )
    .is_none()
    {
        Some(Edit::range_replacement(
            format!("({})", locator.slice(current_branch.test)),
            current_branch.test.range(),
        ))
    } else {
        None
    };

    Ok(Fix::safe_edits(
        deletion_edit,
        parenthesize_edit.into_iter().chain(Some(insertion_edit)),
    ))
}

/// Check if comments can be safely preserved during merge
fn can_preserve_comments_during_merge(
    current_branch: &IfElifBranch,
    following_branch: &IfElifBranch,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) -> bool {
    // Check the entire range from current branch start to following branch end
    // This includes comments between branches, inline comments, etc.
    let merge_range = TextRange::new(current_branch.start(), following_branch.end());
    let comments = comment_ranges.comments_in_range(merge_range);

    for comment_range in comments {
        let comment_text = locator.slice(*comment_range).trim();

        // Skip empty or whitespace-only comments
        if comment_text.is_empty() || comment_text == "#" {
            continue;
        }

        // Check if comment is inline with the test condition by checking if the comment
        // is on the same line as the test end position
        let current_test_end_line_start = locator.line_start(current_branch.test.end());
        let current_test_end_line_end = locator.line_end(current_branch.test.end());
        let current_test_line_range =
            TextRange::new(current_test_end_line_start, current_test_end_line_end);

        let following_test_end_line_start = locator.line_start(following_branch.test.end());
        let following_test_end_line_end = locator.line_end(following_branch.test.end());
        let following_test_line_range =
            TextRange::new(following_test_end_line_start, following_test_end_line_end);

        // If comment is on the same line as either test condition, it can be preserved
        // because it will stay with the merged condition
        if current_test_line_range.contains(comment_range.start())
            || following_test_line_range.contains(comment_range.start())
        {
            continue;
        }

        // Check if comments are in the body and identical between branches
        let current_body_range = body_range(current_branch, locator);
        let following_body_range = body_range(following_branch, locator);

        let is_in_current_body = current_body_range.contains(comment_range.start());
        let is_in_following_body = following_body_range.contains(comment_range.start());

        if is_in_current_body || is_in_following_body {
            // Body comments - check if they're identical between branches
            let current_body_comments: Vec<_> = comment_ranges
                .comments_in_range(current_body_range)
                .iter()
                .map(|range| locator.slice(*range).trim())
                .collect();

            let following_body_comments: Vec<_> = comment_ranges
                .comments_in_range(following_body_range)
                .iter()
                .map(|range| locator.slice(*range).trim())
                .collect();

            // Only allow if body comments are identical
            if current_body_comments != following_body_comments {
                return false;
            }
            continue;
        }

        // If we reach here, there's a standalone comment between branches
        // that could be lost - be conservative
        return false;
    }

    true
}

/// Return the [`TextRange`] of an [`IfElifBranch`]'s body (from the end of the test to the end of
/// the body).
fn body_range(branch: &IfElifBranch, locator: &Locator) -> TextRange {
    TextRange::new(
        locator.line_end(branch.test.end()),
        locator.line_end(branch.end()),
    )
}
