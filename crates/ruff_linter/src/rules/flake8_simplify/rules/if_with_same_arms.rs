use anyhow::Result;

use ast::whitespace::indentation;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableStmt;
use ruff_python_ast::stmt_if::{if_elif_branches, IfElifBranch};
use ruff_python_codegen::Generator;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Combine `if` branches using logical `or` operator")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Combine `if` branches".to_string())
    }
}

/// SIM114
pub(crate) fn if_with_same_arms(checker: &mut Checker, stmt_if: &ast::StmtIf) {
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
            .comments_in_range(body_range(&current_branch, checker.locator()))
            .iter()
            .map(|range| checker.locator().slice(*range));
        let second_comments = checker
            .indexer()
            .comment_ranges()
            .comments_in_range(body_range(following_branch, checker.locator()))
            .iter()
            .map(|range| checker.locator().slice(*range));
        if !first_comments.eq(second_comments) {
            continue;
        }

        let mut diagnostic = Diagnostic::new(
            IfWithSameArms,
            TextRange::new(current_branch.start(), following_branch.end()),
        );

        if checker.settings.preview.is_enabled() {
            diagnostic.try_set_fix(|| {
                merge_branches(
                    &current_branch,
                    &following_branch,
                    checker.locator(),
                    checker.generator(),
                )
            });
        }

        checker.diagnostics.push(diagnostic);
    }
}

/// Generate a [`Fix`] to merge two [`IfElifBranch`] branches.
fn merge_branches(
    current_branch: &IfElifBranch,
    following_branch: &IfElifBranch,
    locator: &Locator,
    generator: Generator,
) -> Result<Fix> {
    let current_branch_colon =
        SimpleTokenizer::starts_at(current_branch.test.end(), locator.contents())
            .find(|token| token.kind == SimpleTokenKind::Colon)
            .unwrap();

    let mut following_branch_tokenizer =
        SimpleTokenizer::starts_at(following_branch.test.end(), locator.contents());

    let following_branch_colon = following_branch_tokenizer
        .find(|token| token.kind == SimpleTokenKind::Colon)
        .unwrap();

    let main_edit = if let Some(following_branch_comment) =
        following_branch_tokenizer.find(|token| token.kind == SimpleTokenKind::Comment)
    {
        let indentation =
            indentation(locator, following_branch.body.first().unwrap()).unwrap_or("");
        Edit::range_replacement(
            format!("{indentation}"),
            TextRange::new(
                locator.full_line_end(current_branch_colon.end()),
                following_branch_comment.start(),
            ),
        )
    } else {
        Edit::deletion(
            locator.full_line_end(current_branch_colon.end()),
            locator.full_line_end(following_branch_colon.end()),
        )
    };

    Ok(Fix::safe_edits(
        main_edit,
        [Edit::insertion(
            format!(" or {}", generator.expr(following_branch.test)),
            current_branch_colon.start(),
        )],
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
