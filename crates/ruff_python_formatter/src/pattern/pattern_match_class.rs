use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Pattern, PatternMatchClass, Ranged};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{TextRange, TextSize};

use crate::comments::{dangling_comments, SourceComment};
use crate::expression::parentheses::{
    empty_parenthesized, parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, item: &PatternMatchClass, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchClass {
            range,
            cls,
            patterns,
            kwd_attrs,
            kwd_patterns,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        // Identify the dangling comments before and after the open parenthesis.
        let (before_parenthesis, after_parenthesis) = if let Some(left_paren) =
            SimpleTokenizer::starts_at(cls.end(), f.context().source())
                .find(|token| token.kind() == SimpleTokenKind::LParen)
        {
            dangling
                .split_at(dangling.partition_point(|comment| comment.start() < left_paren.start()))
        } else {
            (dangling, [].as_slice())
        };

        write!(f, [cls.format(), dangling_comments(before_parenthesis)])?;

        match (patterns.as_slice(), kwd_attrs.as_slice()) {
            ([], []) => {
                // No patterns; render parentheses with any dangling comments.
                write!(f, [empty_parenthesized("(", after_parenthesis, ")")])
            }
            ([pattern], []) => {
                // A single pattern. We need to take care not to re-parenthesize it, since our standard
                // parenthesis detection will false-positive here.
                let parentheses = if is_single_argument_parenthesized(
                    pattern,
                    item.end(),
                    f.context().source(),
                ) {
                    Parentheses::Always
                } else {
                    Parentheses::Never
                };
                write!(
                    f,
                    [
                        parenthesized("(", &pattern.format().with_options(parentheses), ")")
                            .with_dangling_comments(after_parenthesis)
                    ]
                )
            }
            _ => {
                // Multiple patterns: standard logic.
                let items = format_with(|f| {
                    let mut join = f.join_comma_separated(range.end());
                    join.nodes(patterns.iter());
                    for (key, value) in kwd_attrs.iter().zip(kwd_patterns.iter()) {
                        join.entry(
                            key,
                            &format_with(|f| write!(f, [key.format(), text("="), value.format()])),
                        );
                    }
                    join.finish()
                });
                write!(
                    f,
                    [parenthesized("(", &group(&items), ")")
                        .with_dangling_comments(after_parenthesis)]
                )
            }
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for PatternMatchClass {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        // If there are any comments outside of the class parentheses, break:
        // ```python
        // case (
        //     Pattern
        //     # dangling
        //     (...)
        // ): ...
        // ```
        let dangling = context.comments().dangling(self);
        if !dangling.is_empty() {
            if let Some(left_paren) = SimpleTokenizer::starts_at(self.cls.end(), context.source())
                .find(|token| token.kind() == SimpleTokenKind::LParen)
            {
                if dangling
                    .iter()
                    .any(|comment| comment.start() < left_paren.start())
                {
                    return OptionalParentheses::Multiline;
                };
            }
        }
        OptionalParentheses::Never
    }
}

/// Returns `true` if the pattern (which is the only argument to a [`PatternMatchClass`]) is
/// parenthesized. Used to avoid falsely assuming that `x` is parenthesized in cases like:
/// ```python
/// case Point2D(x): ...
/// ```
fn is_single_argument_parenthesized(pattern: &Pattern, call_end: TextSize, source: &str) -> bool {
    let mut has_seen_r_paren = false;
    for token in SimpleTokenizer::new(source, TextRange::new(pattern.end(), call_end)).skip_trivia()
    {
        match token.kind() {
            SimpleTokenKind::RParen => {
                if has_seen_r_paren {
                    return true;
                }
                has_seen_r_paren = true;
            }
            // Skip over any trailing comma
            SimpleTokenKind::Comma => continue,
            _ => {
                // Passed the arguments
                break;
            }
        }
    }
    false
}
