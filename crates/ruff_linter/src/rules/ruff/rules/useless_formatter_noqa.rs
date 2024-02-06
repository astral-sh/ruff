#![allow(dead_code)]
use std::{collections::BTreeMap, fmt::Display};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{AnyNodeRef, Suite};
use ruff_python_trivia::SuppressionKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::{ast::Checker, noqa::delete_comment};

use super::suppression_comment_visitor::{
    CaptureSuppressionComment, SuppressionCommentData, SuppressionCommentVisitor,
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
/// @decorator
/// # fmt: off
/// def example():
///     if True:
///         expression = 1 + \ # fmt: skip
///                      # fmt: off
///                      1
///         # yapf: disable
///     # fmt: on
///     # yapf: enable
/// ```
#[violation]
pub struct UselessFormatterNOQA {
    reason: UselessReason,
}

impl AlwaysFixableViolation for UselessFormatterNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Supression comment is useless - {}", self.reason)
    }

    fn fix_title(&self) -> String {
        format!("Remove this supression comment")
    }
}

/// RUF028
pub(crate) fn useless_formatter_noqa(checker: &mut Checker, suite: &Suite) {
    let indexer = checker.indexer();
    let locator = checker.locator();
    let comment_ranges = indexer.comment_ranges();
    let contents = locator.to_source_code().text();

    let mut comments = UselessSuppressionComments::new();

    let visitor = SuppressionCommentVisitor::new(contents, comment_ranges, &mut comments);

    visitor.visit(suite);

    for (comment, reason) in comments.useless_comments() {
        checker.diagnostics.push(
            Diagnostic::new(UselessFormatterNOQA { reason }, comment.range).with_fix(
                Fix::unsafe_edit(delete_comment(comment.range, checker.locator())),
            ),
        );
    }
}

struct UselessSuppressionComments<'src> {
    captured: BTreeMap<SuppressionCommentData<'src>, Option<UselessReason>>,
}

impl<'src> UselessSuppressionComments<'src> {
    fn new() -> Self {
        Self {
            captured: BTreeMap::default(),
        }
    }
    fn check_suppression_comment(&self, comment: &SuppressionCommentData) -> Option<UselessReason> {
        // Check if the comment is inside of an expression.
        if comment
            .enclosing
            .map(AnyNodeRef::is_expression)
            .unwrap_or_default()
        {
            return Some(UselessReason::InsideExpression);
        }
        if comment.kind == SuppressionKind::Skip && !comment.line_position.is_end_of_line() {
            return Some(UselessReason::SkipHasToBeTrailing);
        }
        // If the comment turns off formatting, we need to make sure
        // that something follows it which is worth formatting.
        if comment.kind == SuppressionKind::Off && comment.following.is_none() {
            return Some(UselessReason::NoCodeSuppressed);
        }
        // If the comment turns on formatting, we need to check if another
        // comment turned formatting off within the same scope.
        if comment.kind == SuppressionKind::On {
            let enclosing_range = comment
                .enclosing
                .map_or(TextRange::new(0u32.into(), u32::MAX.into()), |e| e.range());
            let has_corresponding_fmt_off = self
                .captured
                .iter()
                .rev()
                .filter(|(c, _)| c.enclosing == comment.enclosing)
                .take_while(|(c, _)| c.range.start() >= enclosing_range.start())
                .any(|(c, _)| c.kind == SuppressionKind::Off);
            if !has_corresponding_fmt_off {
                return Some(UselessReason::NoFmtOff);
            }
        }
        None
    }

    fn useless_comments(
        &self,
    ) -> impl Iterator<Item = (&SuppressionCommentData<'src>, UselessReason)> {
        self.captured.iter().filter_map(|(c, r)| Some((c, (*r)?)))
    }
}

impl<'src> CaptureSuppressionComment<'src> for UselessSuppressionComments<'src> {
    fn capture(&mut self, comment: SuppressionCommentData<'src>) {
        let possible_reason = self.check_suppression_comment(&comment);
        self.captured.insert(comment, possible_reason);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UselessReason {
    InsideExpression,
    NoFmtOff,
    NoCodeSuppressed,
    OnOffNotAllowed,
    SkipHasToBeTrailing,
}

impl Display for UselessReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsideExpression => write!(
                f,
                "suppression comments inside expressions are not supported"
            ),
            Self::NoFmtOff => write!(f, "formatting was already enabled here"),
            Self::NoCodeSuppressed => write!(f, "no eligible code is suppressed by this comment"),
            Self::OnOffNotAllowed => write!(f, "on/off suppression comments are not allowed here"),
            Self::SkipHasToBeTrailing => write!(f, "a skip comment has to be at the end of a line"),
        }
    }
}
