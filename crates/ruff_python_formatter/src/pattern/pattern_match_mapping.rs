use ruff_formatter::{format_args, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchMapping;
use ruff_python_ast::{Expr, Identifier, Pattern};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{leading_comments, trailing_comments, SourceComment};
use crate::expression::parentheses::{
    empty_parenthesized, parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchMapping;

impl FormatNodeRule<PatternMatchMapping> for FormatPatternMatchMapping {
    fn fmt_fields(&self, item: &PatternMatchMapping, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchMapping {
            keys,
            patterns,
            rest,
            range: _,
        } = item;

        debug_assert_eq!(keys.len(), patterns.len());

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if keys.is_empty() && rest.is_none() {
            return empty_parenthesized("{", dangling, "}").fmt(f);
        }

        // This node supports three kinds of dangling comments. Most of the complexity originates
        // with the rest pattern (`{**rest}`), since we can have comments around the `**`, but
        // also, the `**rest` itself is not a node (it's an identifier), so comments that trail it
        // are _also_ dangling.
        //
        // Specifically, we have these three sources of dangling comments:
        // ```python
        // {  # "open parenthesis comment"
        //    key: pattern,
        //    **  # end-of-line "double star comment"
        //    # own-line "double star comment"
        //    rest  # end-of-line "after rest comment"
        //    # own-line "after rest comment"
        // }
        // ```
        let (open_parenthesis_comments, double_star_comments, after_rest_comments) =
            if let Some((double_star, rest)) = find_double_star(item, f.context().source()) {
                let (open_parenthesis_comments, dangling) =
                    dangling.split_at(dangling.partition_point(|comment| {
                        comment.line_position().is_end_of_line()
                            && comment.start() < double_star.start()
                    }));
                let (double_star_comments, after_rest_comments) = dangling
                    .split_at(dangling.partition_point(|comment| comment.start() < rest.start()));
                (
                    open_parenthesis_comments,
                    double_star_comments,
                    after_rest_comments,
                )
            } else {
                (dangling, [].as_slice(), [].as_slice())
            };

        let format_pairs = format_with(|f| {
            let mut joiner = f.join_comma_separated(item.end());

            for (key, pattern) in keys.iter().zip(patterns) {
                let key_pattern_pair = KeyPatternPair { key, pattern };
                joiner.entry(&key_pattern_pair, &key_pattern_pair);
            }

            if let Some(identifier) = rest {
                let rest_pattern = RestPattern {
                    identifier,
                    comments: double_star_comments,
                };
                joiner.entry(&rest_pattern, &rest_pattern);
            }

            joiner.finish()?;

            trailing_comments(after_rest_comments).fmt(f)
        });

        parenthesized("{", &format_pairs, "}")
            .with_dangling_comments(open_parenthesis_comments)
            .fmt(f)
    }
}

impl NeedsParentheses for PatternMatchMapping {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}

/// A struct to format the `rest` element of a [`PatternMatchMapping`] (e.g., `{**rest}`).
#[derive(Debug)]
struct RestPattern<'a> {
    identifier: &'a Identifier,
    comments: &'a [SourceComment],
}

impl Ranged for RestPattern<'_> {
    fn range(&self) -> TextRange {
        self.identifier.range()
    }
}

impl Format<PyFormatContext<'_>> for RestPattern<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [
                leading_comments(self.comments),
                token("**"),
                self.identifier.format()
            ]
        )
    }
}

/// A struct to format a key-pattern pair of a [`PatternMatchMapping`] (e.g., `{key: pattern}`).
#[derive(Debug)]
struct KeyPatternPair<'a> {
    key: &'a Expr,
    pattern: &'a Pattern,
}

impl Ranged for KeyPatternPair<'_> {
    fn range(&self) -> TextRange {
        TextRange::new(self.key.start(), self.pattern.end())
    }
}

impl Format<PyFormatContext<'_>> for KeyPatternPair<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [group(&format_args![
                self.key.format(),
                token(":"),
                space(),
                self.pattern.format()
            ])]
        )
    }
}

/// Given a [`PatternMatchMapping`], finds the range of the `**` element in the `rest` pattern,
/// if it exists.
fn find_double_star(pattern: &PatternMatchMapping, source: &str) -> Option<(TextRange, TextRange)> {
    let PatternMatchMapping {
        keys: _,
        patterns,
        rest,
        range: _,
    } = pattern;

    // If there's no `rest` element, there's no `**`.
    let rest = rest.as_ref()?;

    let mut tokenizer =
        SimpleTokenizer::starts_at(patterns.last().map_or(pattern.start(), Ranged::end), source);
    let double_star = tokenizer.find(|token| token.kind() == SimpleTokenKind::DoubleStar)?;

    Some((double_star.range(), rest.range()))
}
