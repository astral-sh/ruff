use std::fmt::Display;

use smallvec::SmallVec;

use ast::{StmtClassDef, StmtFunctionDef};
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, helpers::comment_indentation_after, AnyNodeRef};
use ruff_python_trivia::{indentation_at_offset, SuppressionKind};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::delete_comment;
use crate::Locator;

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
#[derive(ViolationMetadata)]
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
pub(crate) fn ignored_formatter_suppression_comment(checker: &Checker, suite: &ast::Suite) {
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
        checker.report_diagnostic(
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
const fn is_valid_enclosing_node(node: AnyNodeRef) -> bool {
    match node {
        AnyNodeRef::ModModule(_)
        | AnyNodeRef::ModExpression(_)
        | AnyNodeRef::StmtFunctionDef(_)
        | AnyNodeRef::StmtClassDef(_)
        | AnyNodeRef::StmtReturn(_)
        | AnyNodeRef::StmtDelete(_)
        | AnyNodeRef::StmtTypeAlias(_)
        | AnyNodeRef::StmtAssign(_)
        | AnyNodeRef::StmtAugAssign(_)
        | AnyNodeRef::StmtAnnAssign(_)
        | AnyNodeRef::StmtFor(_)
        | AnyNodeRef::StmtWhile(_)
        | AnyNodeRef::StmtIf(_)
        | AnyNodeRef::StmtWith(_)
        | AnyNodeRef::StmtMatch(_)
        | AnyNodeRef::StmtRaise(_)
        | AnyNodeRef::StmtTry(_)
        | AnyNodeRef::StmtAssert(_)
        | AnyNodeRef::StmtImport(_)
        | AnyNodeRef::StmtImportFrom(_)
        | AnyNodeRef::StmtGlobal(_)
        | AnyNodeRef::StmtNonlocal(_)
        | AnyNodeRef::StmtExpr(_)
        | AnyNodeRef::StmtPass(_)
        | AnyNodeRef::StmtBreak(_)
        | AnyNodeRef::StmtContinue(_)
        | AnyNodeRef::StmtIpyEscapeCommand(_)
        | AnyNodeRef::ExceptHandlerExceptHandler(_)
        | AnyNodeRef::MatchCase(_)
        | AnyNodeRef::Decorator(_)
        | AnyNodeRef::ElifElseClause(_) => true,

        AnyNodeRef::ExprBoolOp(_)
        | AnyNodeRef::ExprNamed(_)
        | AnyNodeRef::ExprBinOp(_)
        | AnyNodeRef::ExprUnaryOp(_)
        | AnyNodeRef::ExprLambda(_)
        | AnyNodeRef::ExprIf(_)
        | AnyNodeRef::ExprDict(_)
        | AnyNodeRef::ExprSet(_)
        | AnyNodeRef::ExprListComp(_)
        | AnyNodeRef::ExprSetComp(_)
        | AnyNodeRef::ExprDictComp(_)
        | AnyNodeRef::ExprGenerator(_)
        | AnyNodeRef::ExprAwait(_)
        | AnyNodeRef::ExprYield(_)
        | AnyNodeRef::ExprYieldFrom(_)
        | AnyNodeRef::ExprCompare(_)
        | AnyNodeRef::ExprCall(_)
        | AnyNodeRef::FStringExpressionElement(_)
        | AnyNodeRef::FStringLiteralElement(_)
        | AnyNodeRef::FStringFormatSpec(_)
        | AnyNodeRef::ExprFString(_)
        | AnyNodeRef::ExprStringLiteral(_)
        | AnyNodeRef::ExprBytesLiteral(_)
        | AnyNodeRef::ExprNumberLiteral(_)
        | AnyNodeRef::ExprBooleanLiteral(_)
        | AnyNodeRef::ExprNoneLiteral(_)
        | AnyNodeRef::ExprEllipsisLiteral(_)
        | AnyNodeRef::ExprAttribute(_)
        | AnyNodeRef::ExprSubscript(_)
        | AnyNodeRef::ExprStarred(_)
        | AnyNodeRef::ExprName(_)
        | AnyNodeRef::ExprList(_)
        | AnyNodeRef::ExprTuple(_)
        | AnyNodeRef::ExprSlice(_)
        | AnyNodeRef::ExprIpyEscapeCommand(_)
        | AnyNodeRef::PatternMatchValue(_)
        | AnyNodeRef::PatternMatchSingleton(_)
        | AnyNodeRef::PatternMatchSequence(_)
        | AnyNodeRef::PatternMatchMapping(_)
        | AnyNodeRef::PatternMatchClass(_)
        | AnyNodeRef::PatternMatchStar(_)
        | AnyNodeRef::PatternMatchAs(_)
        | AnyNodeRef::PatternMatchOr(_)
        | AnyNodeRef::PatternArguments(_)
        | AnyNodeRef::PatternKeyword(_)
        | AnyNodeRef::Comprehension(_)
        | AnyNodeRef::Arguments(_)
        | AnyNodeRef::Parameters(_)
        | AnyNodeRef::Parameter(_)
        | AnyNodeRef::ParameterWithDefault(_)
        | AnyNodeRef::Keyword(_)
        | AnyNodeRef::Alias(_)
        | AnyNodeRef::WithItem(_)
        | AnyNodeRef::TypeParams(_)
        | AnyNodeRef::TypeParamTypeVar(_)
        | AnyNodeRef::TypeParamTypeVarTuple(_)
        | AnyNodeRef::TypeParamParamSpec(_)
        | AnyNodeRef::FString(_)
        | AnyNodeRef::StringLiteral(_)
        | AnyNodeRef::BytesLiteral(_)
        | AnyNodeRef::Identifier(_) => false,
    }
}
