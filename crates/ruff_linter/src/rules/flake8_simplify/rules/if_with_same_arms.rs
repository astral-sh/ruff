use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableStmt;
use ruff_python_ast::stmt_if::{if_elif_branches, IfElifBranch};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
#[violation]
pub struct IfWithSameArms;

impl Violation for IfWithSameArms {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Combine `if` branches using logical `or` operator")
    }
}

/// SIM114
pub(crate) fn if_with_same_arms(checker: &mut Checker, locator: &Locator, stmt_if: &ast::StmtIf) {
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
            .zip(following_branch.body.iter())
            .all(|(stmt1, stmt2)| ComparableStmt::from(stmt1) == ComparableStmt::from(stmt2))
        {
            continue;
        }

        // ...and the same comments
        let first_comments = checker
            .indexer()
            .comment_ranges()
            .comments_in_range(body_range(&current_branch, locator))
            .iter()
            .map(|range| locator.slice(*range));
        let second_comments = checker
            .indexer()
            .comment_ranges()
            .comments_in_range(body_range(following_branch, locator))
            .iter()
            .map(|range| locator.slice(*range));
        if !first_comments.eq(second_comments) {
            continue;
        }

        checker.diagnostics.push(Diagnostic::new(
            IfWithSameArms,
            TextRange::new(current_branch.start(), following_branch.end()),
        ));
    }
}

/// Return the [`TextRange`] of an [`IfElifBranch`]'s body (from the end of the test to the end of
/// the body).
fn body_range(branch: &IfElifBranch, locator: &Locator) -> TextRange {
    TextRange::new(
        locator.line_end(branch.test.end()),
        locator.line_end(branch.end()),
    )
}
