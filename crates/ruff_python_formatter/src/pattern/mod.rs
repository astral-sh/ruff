use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::Pattern;
use ruff_python_trivia::CommentRanges;
use ruff_python_trivia::{
    first_non_trivia_token, BackwardsTokenizer, SimpleToken, SimpleTokenKind,
};
use ruff_text_size::Ranged;

use crate::expression::parentheses::{
    parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
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
