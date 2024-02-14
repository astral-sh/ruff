#![allow(dead_code)]
use std::{collections::BTreeMap, fmt::Display};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_trivia::{indentation_at_offset, SuppressionKind};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen};

use crate::checkers::{ast::Checker, noqa::delete_comment};

use super::suppression_comment_visitor::{
    own_line_comment_indentation, CaptureSuppressionComment, SuppressionCommentData,
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
        format!("Suppression comment is useless - {}", self.reason)
    }

    fn fix_title(&self) -> String {
        format!("Remove this suppression comment")
    }
}

/// RUF028
pub(crate) fn useless_formatter_noqa(checker: &mut Checker, suite: &ast::Suite) {
    let indexer = checker.indexer();
    let locator = checker.locator();
    let comment_ranges = indexer.comment_ranges();

    let mut comments = UselessSuppressionComments::new(locator);

    let visitor = SuppressionCommentVisitor::new(comment_ranges, &mut comments, checker.locator());

    visitor.visit(suite);

    for (comment, reason) in comments.useless_comments() {
        checker.diagnostics.push(
            Diagnostic::new(UselessFormatterNOQA { reason }, comment.range).with_fix(
                Fix::unsafe_edit(delete_comment(comment.range, checker.locator())),
            ),
        );
    }
}

struct UselessSuppressionComments<'src, 'loc> {
    captured: BTreeMap<SuppressionCommentData<'src>, UselessReason>,
    comments_in_scope: Vec<(Option<AnyNodeRef<'src>>, SuppressionKind)>,
    locator: &'loc Locator<'src>,
}

impl<'src, 'loc> UselessSuppressionComments<'src, 'loc> {
    fn new(locator: &'loc Locator<'src>) -> Self {
        Self {
            captured: BTreeMap::default(),
            comments_in_scope: vec![],
            locator,
        }
    }
    /// This function determines whether or not `comment` is a useful suppression comment.
    /// If it isn't, it will give a reason why the comment is useless. See [`UselessReason`] for more.
    fn check_suppression_comment(
        &self,
        comment: &SuppressionCommentData,
    ) -> Result<Option<SuppressionKind>, UselessReason> {
        // check if the comment is inside of an expression.
        if comment
            .enclosing
            .map(AnyNodeRef::is_expression)
            .unwrap_or_default()
        {
            return Err(UselessReason::InsideExpression);
        }

        // check if a skip comment is at the end of a line
        if comment.kind == SuppressionKind::Skip && !comment.line_position.is_end_of_line() {
            return Err(UselessReason::SkipHasToBeTrailing);
        }

        if comment.kind == SuppressionKind::Off && comment.line_position.is_own_line() {
            // check for a previous `fmt: off`
            if comment.previous_state == Some(SuppressionKind::Off) {
                return Err(UselessReason::FmtOffUsedEarlier);
            }
            let Some(following) = comment.following else {
                return Err(UselessReason::NoCodeSuppressed);
            };
            if let Some(enclosing) = comment.enclosing {
                // check if this comment is dangling (in other words, in a block with nothing following it)

                // check if this comment is before an alternative body (for example: an `else` or `elif`)
                if let Some(preceding) = comment.preceding {
                    if is_first_statement_in_alternate_body(following, enclosing) {
                        // check indentation
                        let comment_indentation =
                            own_line_comment_indentation(preceding, comment.range, self.locator);

                        let preceding_indentation =
                            indentation_at_offset(preceding.start(), self.locator)
                                .unwrap_or_default()
                                .text_len();
                        if comment_indentation <= preceding_indentation {
                            return Err(UselessReason::FmtOffOverElseBlock);
                        }
                    }
                }
            }
        }

        if comment.kind == SuppressionKind::On {
            // Ensure the comment is not a trailing comment
            if !comment.line_position.is_own_line() {
                return Err(UselessReason::FmtOnCannotBeTrailing);
            }

            // If the comment turns on formatting, we need to check if another
            // comment turned formatting off within the same scope.
            match comment.previous_state {
                None | Some(SuppressionKind::On) => return Err(UselessReason::NoFmtOff),
                _ => {}
            }
        }

        if comment.kind == SuppressionKind::Off || comment.kind == SuppressionKind::On {
            if let Some(AnyNodeRef::StmtClassDef(class_def)) = comment.enclosing {
                if comment.line_position.is_own_line() && comment.start() < class_def.name.start() {
                    if let Some(decorator) = class_def.decorator_list.last() {
                        if decorator.end() < comment.start() {
                            return Err(UselessReason::BetweenDecorators);
                        }
                    }
                }
            }

            // at this point, any comment being handled should be considered 'valid'.
            // on/off suppression comments should be added to the scope
            return Ok(Some(comment.kind));
        }
        Ok(None)
    }

    fn useless_comments(
        &self,
    ) -> impl Iterator<Item = (&SuppressionCommentData<'src>, UselessReason)> {
        self.captured.iter().map(|(c, r)| (c, *r))
    }
}

impl<'src, 'loc> CaptureSuppressionComment<'src> for UselessSuppressionComments<'src, 'loc> {
    fn capture(&mut self, comment: SuppressionCommentData<'src>) -> Option<SuppressionKind> {
        match self.check_suppression_comment(&comment) {
            Ok(kind) => kind,
            Err(reason) => {
                self.captured.insert(comment, reason);
                None
            }
        }
    }
}

/// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
fn is_first_statement_in_alternate_body(statement: AnyNodeRef, has_body: AnyNodeRef) -> bool {
    match has_body {
        AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
        | AnyNodeRef::StmtWhile(ast::StmtWhile { orelse, .. }) => {
            are_same_optional(statement, orelse.first())
        }

        AnyNodeRef::StmtTry(ast::StmtTry {
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            are_same_optional(statement, handlers.first())
                || are_same_optional(statement, orelse.first())
                || are_same_optional(statement, finalbody.first())
        }

        AnyNodeRef::StmtIf(ast::StmtIf {
            elif_else_clauses, ..
        }) => are_same_optional(statement, elif_else_clauses.first()),
        _ => false,
    }
}

/// Returns `true` if the parameters are parenthesized (as in a function definition), or `false` if
/// not (as in a lambda).
fn are_parameters_parenthesized(parameters: &ast::Parameters, contents: &str) -> bool {
    // A lambda never has parentheses around its parameters, but a function definition always does.
    contents[parameters.range()].starts_with('(')
}

/// Returns `true` if `right` is `Some` and `left` and `right` are referentially equal.
fn are_same_optional<'a, T>(left: AnyNodeRef, right: Option<T>) -> bool
where
    T: Into<AnyNodeRef<'a>>,
{
    right.is_some_and(|right| left.ptr_eq(right.into()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UselessReason {
    InsideExpression,
    FmtOffUsedEarlier,
    NoFmtOff,
    NoCodeSuppressed,
    BetweenDecorators,
    FmtOnCannotBeTrailing,
    SkipHasToBeTrailing,
    FmtOffOverElseBlock,
}

impl Display for UselessReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsideExpression => write!(
                f,
                "suppression comments inside expressions are not supported"
            ),
            Self::FmtOffUsedEarlier => write!(f, "formatting is already disabled here"),
            Self::NoFmtOff => write!(f, "formatting is already enabled here"),
            Self::NoCodeSuppressed => write!(f, "no eligible code is suppressed by this comment"),
            Self::BetweenDecorators => {
                write!(f, "suppression comment cannot be between decorators")
            }
            Self::SkipHasToBeTrailing => {
                write!(f, "'fmt: skip' has to be at the end of a line")
            }
            Self::FmtOnCannotBeTrailing => {
                write!(f, "'fmt: on' cannot be at the end of a line")
            }
            Self::FmtOffOverElseBlock => {
                write!(f, "'fmt: off' cannot be in front of an else/elif")
            }
        }
    }
}
