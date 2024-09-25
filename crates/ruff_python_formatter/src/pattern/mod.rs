use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{MatchCase, Pattern};
use ruff_python_trivia::CommentRanges;
use ruff_python_trivia::{
    first_non_trivia_token, BackwardsTokenizer, SimpleToken, SimpleTokenKind,
};
use ruff_text_size::Ranged;

use crate::builders::parenthesize_if_expands;
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{
    optional_parentheses, parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
};
use crate::prelude::*;

pub(crate) mod pattern_arguments;
pub(crate) mod pattern_keyword;
pub(crate) mod pattern_match_as;
pub(crate) mod pattern_match_class;
pub(crate) mod pattern_match_mapping;
pub(crate) mod pattern_match_or;
pub(crate) mod pattern_match_sequence;
pub(crate) mod pattern_match_singleton;
pub(crate) mod pattern_match_star;
pub(crate) mod pattern_match_value;

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub struct FormatPattern {
    parentheses: Parentheses,
}

impl FormatRuleWithOptions<Pattern, PyFormatContext<'_>> for FormatPattern {
    type Options = Parentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatRule<Pattern, PyFormatContext<'_>> for FormatPattern {
    fn fmt(&self, pattern: &Pattern, f: &mut PyFormatter) -> FormatResult<()> {
        let format_pattern = format_with(|f| match pattern {
            Pattern::MatchValue(pattern) => pattern.format().fmt(f),
            Pattern::MatchSingleton(pattern) => pattern.format().fmt(f),
            Pattern::MatchSequence(pattern) => pattern.format().fmt(f),
            Pattern::MatchMapping(pattern) => pattern.format().fmt(f),
            Pattern::MatchClass(pattern) => pattern.format().fmt(f),
            Pattern::MatchStar(pattern) => pattern.format().fmt(f),
            Pattern::MatchAs(pattern) => pattern.format().fmt(f),
            Pattern::MatchOr(pattern) => pattern.format().fmt(f),
        });

        let parenthesize = match self.parentheses {
            Parentheses::Preserve => is_pattern_parenthesized(
                pattern,
                f.context().comments().ranges(),
                f.context().source(),
            ),
            Parentheses::Always => true,
            Parentheses::Never => false,
        };

        if parenthesize {
            let comments = f.context().comments().clone();

            // Any comments on the open parenthesis.
            //
            // For example, `# comment` in:
            // ```python
            // (  # comment
            //    1
            // )
            // ```
            let open_parenthesis_comment = comments
                .leading(pattern)
                .first()
                .filter(|comment| comment.line_position().is_end_of_line());

            parenthesized("(", &format_pattern, ")")
                .with_dangling_comments(
                    open_parenthesis_comment
                        .map(std::slice::from_ref)
                        .unwrap_or_default(),
                )
                .fmt(f)
        } else {
            format_pattern.fmt(f)
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Pattern {
    type Format<'a> = FormatRefWithRule<'a, Pattern, FormatPattern, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatPattern::default())
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Pattern {
    type Format = FormatOwnedWithRule<Pattern, FormatPattern, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatPattern::default())
    }
}

fn is_pattern_parenthesized(
    pattern: &Pattern,
    comment_ranges: &CommentRanges,
    contents: &str,
) -> bool {
    // First test if there's a closing parentheses because it tends to be cheaper.
    if matches!(
        first_non_trivia_token(pattern.end(), contents),
        Some(SimpleToken {
            kind: SimpleTokenKind::RParen,
            ..
        })
    ) {
        matches!(
            BackwardsTokenizer::up_to(pattern.start(), contents, comment_ranges)
                .skip_trivia()
                .next(),
            Some(SimpleToken {
                kind: SimpleTokenKind::LParen,
                ..
            })
        )
    } else {
        false
    }
}

impl NeedsParentheses for Pattern {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        match self {
            Pattern::MatchValue(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchSingleton(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchSequence(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchMapping(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchClass(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchStar(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchAs(pattern) => pattern.needs_parentheses(parent, context),
            Pattern::MatchOr(pattern) => pattern.needs_parentheses(parent, context),
        }
    }
}

pub(crate) fn maybe_parenthesize_pattern<'a>(
    pattern: &'a Pattern,
    case: &'a MatchCase,
) -> MaybeParenthesizePattern<'a> {
    MaybeParenthesizePattern { pattern, case }
}

#[derive(Debug)]
pub(crate) struct MaybeParenthesizePattern<'a> {
    pattern: &'a Pattern,
    case: &'a MatchCase,
}

impl Format<PyFormatContext<'_>> for MaybeParenthesizePattern<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let MaybeParenthesizePattern { pattern, case } = self;

        let comments = f.context().comments();
        let pattern_comments = comments.leading_dangling_trailing(*pattern);

        // If the pattern has comments, we always want to preserve the parentheses. This also
        // ensures that we correctly handle parenthesized comments, and don't need to worry about
        // them in the implementation below.
        if pattern_comments.has_leading() || pattern_comments.has_trailing_own_line() {
            return pattern.format().with_options(Parentheses::Always).fmt(f);
        }

        let needs_parentheses = pattern.needs_parentheses(AnyNodeRef::from(*case), f.context());

        match needs_parentheses {
            OptionalParentheses::Always => {
                pattern.format().with_options(Parentheses::Always).fmt(f)
            }
            OptionalParentheses::Never => pattern.format().with_options(Parentheses::Never).fmt(f),
            OptionalParentheses::Multiline => {
                if can_pattern_omit_optional_parentheses(pattern, f.context()) {
                    optional_parentheses(&pattern.format().with_options(Parentheses::Never)).fmt(f)
                } else {
                    parenthesize_if_expands(&pattern.format().with_options(Parentheses::Never))
                        .fmt(f)
                }
            }
            OptionalParentheses::BestFit => {
                if pattern_comments.has_trailing() {
                    pattern.format().with_options(Parentheses::Always).fmt(f)
                } else {
                    // The group id is necessary because the nested expressions may reference it.
                    let group_id = f.group_id("optional_parentheses");
                    let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

                    best_fit_parenthesize(&pattern.format().with_options(Parentheses::Never))
                        .with_group_id(Some(group_id))
                        .fmt(f)
                }
            }
        }
    }
}

pub(crate) fn can_pattern_omit_optional_parentheses(
    _pattern: &Pattern,
    _context: &PyFormatContext,
) -> bool {
    // TODO Implement
    false
}
