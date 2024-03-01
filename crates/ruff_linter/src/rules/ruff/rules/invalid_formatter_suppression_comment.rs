use std::fmt::Display;

use ast::{StmtClassDef, StmtFunctionDef};
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, helpers::comment_indentation_after, AnyNodeRef};
use ruff_python_trivia::{indentation_at_offset, SuppressionKind};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange};
use smallvec::SmallVec;

use crate::checkers::ast::Checker;
use crate::fix::edits::delete_comment;

use super::suppression_comment_visitor::{
    CaptureSuppressionComment, SuppressionComment, SuppressionCommentData,
    SuppressionCommentVisitor,
};

/// ## What it does
/// Checks for formatter suppression comments that are ineffective or incompatible
/// with Ruff's formatter.
///
/// ## Why is this bad?
/// Suppression comments that do not actually prevent formatting could cause unintended changes
/// when the formatter is run.
///
/// ## Examples
/// In the following example, all suppression comments would cause
/// a rule violation.
///
/// ```python
/// def decorator():
///     pass
///
///
/// @decorator
/// # fmt: off
/// def example():
///     if True:
///         # fmt: skip
///         expression = [
///             # fmt: off
///             1,
///             2,
///         ]
///         # yapf: disable
///     # fmt: on
///     # yapf: enable
/// ```
#[violation]
pub struct InvalidFormatterSuppressionComment {
    reason: IgnoredReason,
}

impl AlwaysFixableViolation for InvalidFormatterSuppressionComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "This suppression comment is invalid because {}",
            self.reason
        )
    }

    fn fix_title(&self) -> String {
        format!("Remove this comment")
    }
}

/// RUF028
pub(crate) fn ignored_formatter_suppression_comment(checker: &mut Checker, suite: &ast::Suite) {
    let indexer = checker.indexer();
    let locator = checker.locator();
    let comment_ranges: SmallVec<[SuppressionComment; 8]> = indexer
        .comment_ranges()
        .into_iter()
        .filter_map(|range| {
            Some(SuppressionComment {
                range: *range,
                kind: SuppressionKind::from_comment(locator.slice(range))?,
            })
        })
        .collect();

    if comment_ranges.is_empty() {
        return;
    }

    let mut comments = UselessSuppressionComments::new(locator);

    let visitor = SuppressionCommentVisitor::new(
        comment_ranges.into_iter(),
        &mut comments,
        checker.locator(),
    );

    visitor.visit(suite);

    comments.sort();

    for (range, reason) in comments.ignored_comments() {
        checker.diagnostics.push(
            Diagnostic::new(InvalidFormatterSuppressionComment { reason }, range)
                .with_fix(Fix::unsafe_edit(delete_comment(range, checker.locator()))),
        );
    }
}

struct UselessSuppressionComments<'src, 'loc> {
    captured: Vec<(TextRange, IgnoredReason)>,
    locator: &'loc Locator<'src>,
}

impl<'src, 'loc> UselessSuppressionComments<'src, 'loc> {
    fn new(locator: &'loc Locator<'src>) -> Self {
        Self {
            captured: vec![],
            locator,
        }
    }
    /// This function determines whether or not `comment` is a useful suppression comment.
    /// If it isn't, it will give a reason why the comment is ignored. See [`IgnoredReason`] for more.
    fn check_suppression_comment(
        &self,
        comment: &SuppressionCommentData,
    ) -> Result<(), IgnoredReason> {
        // check if the comment is inside of an expression.
        if comment
            .enclosing
            .map(|n| !AnyNodeRef::is_statement(n))
            .unwrap_or_default()
        {
            return Err(IgnoredReason::InNonStatement);
        }

        // check if a skip comment is at the end of a line
        if comment.kind == SuppressionKind::Skip && !comment.line_position.is_end_of_line() {
            return Err(IgnoredReason::SkipHasToBeTrailing);
        }

        if comment.kind == SuppressionKind::Off || comment.kind == SuppressionKind::On {
            if let Some(
                AnyNodeRef::StmtClassDef(StmtClassDef {
                    name,
                    decorator_list,
                    ..
                })
                | AnyNodeRef::StmtFunctionDef(StmtFunctionDef {
                    name,
                    decorator_list,
                    ..
                }),
            ) = comment.enclosing
            {
                if comment.line_position.is_own_line() && comment.range.start() < name.start() {
                    if let Some(decorator) = decorator_list.first() {
                        if decorator.end() < comment.range.start() {
                            return Err(IgnoredReason::AfterDecorator);
                        }
                    }
                }
            }
        }

        if comment.kind == SuppressionKind::Off && comment.line_position.is_own_line() {
            if let (Some(enclosing), Some(preceding), Some(following)) =
                (comment.enclosing, comment.preceding, comment.following)
            {
                if following.is_first_statement_in_alternate_body(enclosing) {
                    // check indentation
                    let comment_indentation =
                        comment_indentation_after(preceding, comment.range, self.locator);

                    let preceding_indentation =
                        indentation_at_offset(preceding.start(), self.locator)
                            .unwrap_or_default()
                            .text_len();
                    if comment_indentation != preceding_indentation {
                        return Err(IgnoredReason::FmtOffAboveBlock);
                    }
                }
            }
        }

        if comment.kind == SuppressionKind::On {
            // Ensure the comment is not a trailing comment
            if !comment.line_position.is_own_line() {
                return Err(IgnoredReason::FmtOnCannotBeTrailing);
            }
        }

        Ok(())
    }

    fn sort(&mut self) {
        self.captured.sort_by_key(|(t, _)| t.start());
    }

    fn ignored_comments(&self) -> impl Iterator<Item = (TextRange, IgnoredReason)> + '_ {
        self.captured.iter().map(|(r, i)| (*r, *i))
    }
}

impl<'src, 'loc> CaptureSuppressionComment<'src> for UselessSuppressionComments<'src, 'loc> {
    fn capture(&mut self, comment: SuppressionCommentData<'src>) {
        match self.check_suppression_comment(&comment) {
            Ok(()) => {}
            Err(reason) => {
                self.captured.push((comment.range, reason));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IgnoredReason {
    InNonStatement,
    AfterDecorator,
    SkipHasToBeTrailing,
    FmtOnCannotBeTrailing,
    FmtOffAboveBlock,
}

impl Display for IgnoredReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InNonStatement => write!(
                f,
                "it cannot be in an expression, pattern, argument list, or other non-statement"
            ),
            Self::AfterDecorator => {
                write!(f, "it cannot be after a decorator")
            }
            Self::SkipHasToBeTrailing => {
                write!(f, "it cannot be on its own line")
            }
            Self::FmtOnCannotBeTrailing => {
                write!(f, "it cannot be at the end of a line")
            }
            Self::FmtOffAboveBlock => {
                write!(f, "it cannot be directly above an alternate body")
            }
        }
    }
}
