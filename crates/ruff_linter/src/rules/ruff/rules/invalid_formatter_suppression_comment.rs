use std::fmt::Display;

use smallvec::SmallVec;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    flat::AnyNodeRef,
    flat::{self, fields as ast},
    helpers::comment_indentation_after,
};
use ruff_python_trivia::{SuppressionKind, indentation_at_offset};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::delete_comment;
use crate::{AlwaysFixableViolation, Fix};

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
/// ## Example
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
///
/// ## Fix safety
///
/// This fix is always marked as unsafe because it deletes the invalid suppression comment,
/// rather than trying to move it to a valid position, which the user more likely intended.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.12.0", category = Category::Suspicious)]
pub(crate) struct InvalidFormatterSuppressionComment {
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
        "Remove this comment".to_string()
    }
}

/// RUF028
pub(crate) fn ignored_formatter_suppression_comment(checker: &Checker, suite: flat::Suite<'_>) {
    let locator = checker.locator();
    let comment_ranges: SmallVec<[SuppressionComment; 8]> = checker
        .comment_ranges()
        .into_iter()
        .filter_map(|range| {
            Some(SuppressionComment {
                range,
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
        checker
            .report_diagnostic(InvalidFormatterSuppressionComment { reason }, range)
            .set_fix(Fix::unsafe_edit(delete_comment(range, checker.locator())));
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
            .is_some_and(|n| !is_valid_enclosing_node(n))
        {
            return Err(IgnoredReason::InNonStatement);
        }

        // check if a skip comment is at the end of a line
        if comment.kind == SuppressionKind::Skip && !comment.line_position.is_end_of_line() {
            return Err(IgnoredReason::SkipHasToBeTrailing);
        }

        if comment.kind == SuppressionKind::Off || comment.kind == SuppressionKind::On {
            if let Some(
                ast::AnyFields::StmtClassDef(ast::StmtClassDef {
                    name,
                    decorator_list,
                    ..
                })
                | ast::AnyFields::StmtFunctionDef(ast::StmtFunctionDef {
                    name,
                    decorator_list,
                    ..
                }),
            ) = comment.enclosing.map(flat::View::<flat::Any>::fields)
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
                    let comment_indentation = comment_indentation_after(
                        preceding,
                        comment.range,
                        self.locator.contents(),
                    );

                    let preceding_indentation =
                        indentation_at_offset(preceding.start(), self.locator.contents())
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

impl<'src> CaptureSuppressionComment<'src> for UselessSuppressionComments<'src, '_> {
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

/// Checks if an enclosing node is allowed to enclose a suppression comment.
fn is_valid_enclosing_node(node: AnyNodeRef) -> bool {
    match node.classify() {
        flat::AnyKind::ModModule(_)
        | flat::AnyKind::ModExpression(_)
        | flat::AnyKind::StmtFunctionDef(_)
        | flat::AnyKind::StmtClassDef(_)
        | flat::AnyKind::StmtReturn(_)
        | flat::AnyKind::StmtDelete(_)
        | flat::AnyKind::StmtTypeAlias(_)
        | flat::AnyKind::StmtAssign(_)
        | flat::AnyKind::StmtAugAssign(_)
        | flat::AnyKind::StmtAnnAssign(_)
        | flat::AnyKind::StmtFor(_)
        | flat::AnyKind::StmtWhile(_)
        | flat::AnyKind::StmtIf(_)
        | flat::AnyKind::StmtWith(_)
        | flat::AnyKind::StmtMatch(_)
        | flat::AnyKind::StmtRaise(_)
        | flat::AnyKind::StmtTry(_)
        | flat::AnyKind::StmtAssert(_)
        | flat::AnyKind::StmtImport(_)
        | flat::AnyKind::StmtImportFrom(_)
        | flat::AnyKind::StmtGlobal(_)
        | flat::AnyKind::StmtNonlocal(_)
        | flat::AnyKind::StmtExpr(_)
        | flat::AnyKind::StmtPass(_)
        | flat::AnyKind::StmtBreak(_)
        | flat::AnyKind::StmtContinue(_)
        | flat::AnyKind::StmtIpyEscapeCommand(_)
        | flat::AnyKind::ExceptHandlerExceptHandler(_)
        | flat::AnyKind::MatchCase(_)
        | flat::AnyKind::Decorator(_)
        | flat::AnyKind::ElifElseClause(_) => true,

        flat::AnyKind::ExprBoolOp(_)
        | flat::AnyKind::ExprNamed(_)
        | flat::AnyKind::ExprBinOp(_)
        | flat::AnyKind::ExprUnaryOp(_)
        | flat::AnyKind::ExprLambda(_)
        | flat::AnyKind::ExprIf(_)
        | flat::AnyKind::ExprDict(_)
        | flat::AnyKind::ExprSet(_)
        | flat::AnyKind::ExprListComp(_)
        | flat::AnyKind::ExprSetComp(_)
        | flat::AnyKind::ExprDictComp(_)
        | flat::AnyKind::ExprGenerator(_)
        | flat::AnyKind::ExprAwait(_)
        | flat::AnyKind::ExprYield(_)
        | flat::AnyKind::ExprYieldFrom(_)
        | flat::AnyKind::ExprCompare(_)
        | flat::AnyKind::ExprCall(_)
        | flat::AnyKind::InterpolatedElement(_)
        | flat::AnyKind::InterpolatedStringLiteralElement(_)
        | flat::AnyKind::InterpolatedStringFormatSpec(_)
        | flat::AnyKind::ExprFString(_)
        | flat::AnyKind::ExprTString(_)
        | flat::AnyKind::ExprStringLiteral(_)
        | flat::AnyKind::ExprBytesLiteral(_)
        | flat::AnyKind::ExprNumberLiteral(_)
        | flat::AnyKind::ExprBooleanLiteral(_)
        | flat::AnyKind::ExprNoneLiteral(_)
        | flat::AnyKind::ExprEllipsisLiteral(_)
        | flat::AnyKind::ExprAttribute(_)
        | flat::AnyKind::ExprSubscript(_)
        | flat::AnyKind::ExprStarred(_)
        | flat::AnyKind::ExprName(_)
        | flat::AnyKind::ExprList(_)
        | flat::AnyKind::ExprTuple(_)
        | flat::AnyKind::ExprSlice(_)
        | flat::AnyKind::ExprIpyEscapeCommand(_)
        | flat::AnyKind::PatternMatchValue(_)
        | flat::AnyKind::PatternMatchSingleton(_)
        | flat::AnyKind::PatternMatchSequence(_)
        | flat::AnyKind::PatternMatchMapping(_)
        | flat::AnyKind::PatternMatchClass(_)
        | flat::AnyKind::PatternMatchStar(_)
        | flat::AnyKind::PatternMatchAs(_)
        | flat::AnyKind::PatternMatchOr(_)
        | flat::AnyKind::PatternArguments(_)
        | flat::AnyKind::PatternKeyword(_)
        | flat::AnyKind::Comprehension(_)
        | flat::AnyKind::Arguments(_)
        | flat::AnyKind::Parameters(_)
        | flat::AnyKind::Parameter(_)
        | flat::AnyKind::ParameterWithDefault(_)
        | flat::AnyKind::Keyword(_)
        | flat::AnyKind::Alias(_)
        | flat::AnyKind::WithItem(_)
        | flat::AnyKind::TypeParams(_)
        | flat::AnyKind::TypeParamTypeVar(_)
        | flat::AnyKind::TypeParamTypeVarTuple(_)
        | flat::AnyKind::TypeParamParamSpec(_)
        | flat::AnyKind::FString(_)
        | flat::AnyKind::TString(_)
        | flat::AnyKind::StringLiteral(_)
        | flat::AnyKind::BytesLiteral(_)
        | flat::AnyKind::Identifier(_) => false,
    }
}
