use std::assert_matches;

use ruff_formatter::{FormatRuleWithOptions, format_args, write};
use ruff_python_ast::{AnyNodeRef, ParameterItem, Parameters};
use ruff_python_trivia::{CommentLinePosition, SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::comments::{
    SourceComment, dangling_comments, dangling_open_parenthesis_comments, leading_comments,
    leading_node_comments, trailing_comments,
};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::empty_parenthesized;
use crate::prelude::*;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum ParametersParentheses {
    /// By default, parameters will always preserve their surrounding parentheses.
    #[default]
    Preserve,

    /// Handle special cases where parentheses should never be used.
    ///
    /// An example where parentheses are never used for parameters would be with lambda
    /// expressions. The following is invalid syntax:
    /// ```python
    /// lambda (x, y, z): ...
    /// ```
    /// Instead the lambda here should be:
    /// ```python
    /// lambda x, y, z: ...
    /// ```
    Never,
}

#[derive(Default)]
pub struct FormatParameters {
    parentheses: ParametersParentheses,
}

impl FormatRuleWithOptions<Parameters, PyFormatContext<'_>> for FormatParameters {
    type Options = ParametersParentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatNodeRule<Parameters> for FormatParameters {
    fn fmt_fields(&self, item: &Parameters, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        // First dangling comment: trailing the opening parenthesis, e.g.:
        // ```python
        // def f(  # comment
        //     x,
        //     y,
        //     z,
        // ): ...
        // TODO(charlie): We already identified this comment as such in placement.rs. Consider
        // labeling it as such. See: https://github.com/astral-sh/ruff/issues/5247.
        let parenthesis_comments_end = usize::from(dangling.first().is_some_and(|comment| {
            if comment.line_position().is_end_of_line() {
                // Ensure that there are no tokens between the open bracket and the comment.
                let mut lexer = SimpleTokenizer::new(
                    f.context().source(),
                    TextRange::new(item.start(), comment.start()),
                )
                .skip_trivia()
                .skip_while(|t| {
                    matches!(
                        t.kind(),
                        SimpleTokenKind::LParen
                            | SimpleTokenKind::LBrace
                            | SimpleTokenKind::LBracket
                    )
                });
                if lexer.next().is_none() {
                    return true;
                }
            }
            false
        }));

        // Separate into (dangling comments on the open parenthesis) and (dangling comments on the
        // argument separators, e.g., `*` or `/`).
        let (parenthesis_dangling, parameters_dangling) =
            dangling.split_at(parenthesis_comments_end);

        let format_inner = format_with(|f: &mut PyFormatter| {
            let separator = format_with(|f: &mut PyFormatter| {
                token(",").fmt(f)?;

                if f.context().node_level().is_parenthesized() {
                    soft_line_break_or_space().fmt(f)
                } else {
                    space().fmt(f)
                }
            });
            let source = f.context().source();
            let mut joiner = f.join_with(separator);
            let mut last_node: Option<AnyNodeRef> = None;

            let mut items = item.items_in_binding_order().peekable();
            let mut need_star = true;
            let mut slash_comments_end = 0;
            let mut ends_with_slash = false;

            while let Some(parameter) = items.next() {
                ends_with_slash = false;
                let node = match parameter {
                    ParameterItem::PositionalOnly(p) => {
                        joiner.entry(&p.format());
                        if !matches!(items.peek(), Some(ParameterItem::PositionalOnly(_))) {
                            ends_with_slash = true;
                            if !parameters_dangling.is_empty() {
                                let following_start =
                                    items.peek().map_or(item.end(), |p| p.as_ref().start());
                                // A bare star can share this gap, as in `def f(a, /, *, b)`.
                                let (slash, _) = find_parameter_separators(
                                    source,
                                    TextRange::new(p.end(), following_start),
                                );
                                slash_comments_end =
                                    parameters_dangling.partition_point(|comment| {
                                        assign_argument_separator_comment_placement(
                                            slash.as_ref(),
                                            None,
                                            comment.range(),
                                            comment.line_position(),
                                        )
                                        .is_some()
                                    });
                            }
                            joiner.entry(&CommentsAroundText {
                                text: "/",
                                comments: &parameters_dangling[..slash_comments_end],
                            });
                        }
                        p.into()
                    }
                    ParameterItem::PositionalOrKeyword(p) => {
                        joiner.entry(&p.format());
                        p.into()
                    }
                    ParameterItem::Vararg(p) => {
                        need_star = false;
                        joiner.entry(&format_args![
                            leading_node_comments(p),
                            token("*"),
                            p.format()
                        ]);
                        p.into()
                    }
                    ParameterItem::KeywordOnly(p) => {
                        if need_star {
                            need_star = false;
                            // Given very strange comment placement, comments here may not actually have been
                            // marked as `StarLeading`/`StarTrailing`, but that's fine since we still produce
                            // a stable formatting in this case
                            // ```python
                            // def f42(
                            //     a,
                            //     / # 1
                            //     # 2
                            //     , # 3
                            //     # 4
                            //     * # 5
                            //     , # 6
                            //     c,
                            // ):
                            //     pass
                            // ```
                            joiner.entry(&CommentsAroundText {
                                text: "*",
                                comments: &parameters_dangling[slash_comments_end..],
                            });
                        }
                        joiner.entry(&p.format());
                        p.into()
                    }
                    ParameterItem::Kwarg(p) => {
                        joiner.entry(&format_args![
                            leading_node_comments(p),
                            token("**"),
                            p.format()
                        ]);
                        p.into()
                    }
                };
                last_node = Some(node);
            }

            joiner.finish()?;

            // Functions use the regular magic trailing comma logic, lambdas may or may not have
            // a trailing comma but it's just preserved without any magic.
            // ```python
            // # Add magic trailing comma if its expands
            // def f(a): pass
            // # Expands if magic trailing comma setting is respect, otherwise remove the comma
            // def g(a,): pass
            // # Never expands
            // x1 = lambda y: 1
            // # Never expands, the comma is always preserved
            // x2 = lambda y,: 1
            // ```
            if self.parentheses == ParametersParentheses::Never {
                // For lambdas (no parentheses), preserve the trailing comma. It doesn't
                // behave like a magic trailing comma, it's just preserved
                if has_trailing_comma(last_node, ends_with_slash, source) {
                    write!(f, [token(",")])?;
                }
            } else {
                write!(f, [if_group_breaks(&token(","))])?;

                if f.options().magic_trailing_comma().is_respect()
                    && has_trailing_comma(last_node, ends_with_slash, source)
                {
                    // Make the magic trailing comma expand the group
                    write!(f, [hard_line_break()])?;
                }
            }

            Ok(())
        });

        let num_parameters = item.len();

        if self.parentheses == ParametersParentheses::Never {
            write!(f, [group(&format_inner), dangling_comments(dangling)])
        } else if num_parameters == 0 {
            let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);
            // No parameters, format any dangling comments between `()`
            write!(f, [empty_parenthesized("(", dangling, ")")])
        } else if num_parameters == 1
            && item.posonlyargs().next().is_none()
            && item.kwonlyargs().next().is_none()
        {
            // If we have a single argument, avoid the inner group, to ensure that we insert a
            // trailing comma if the outer group breaks.
            let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);
            write!(
                f,
                [
                    token("("),
                    dangling_open_parenthesis_comments(parenthesis_dangling),
                    soft_block_indent(&format_inner),
                    token(")")
                ]
            )
        } else {
            // Intentionally avoid `parenthesized`, which groups the entire formatted contents.
            // We want parameters to be grouped alongside return types, one level up, so we
            // format them "inline" here.
            let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);
            write!(
                f,
                [
                    token("("),
                    dangling_open_parenthesis_comments(parenthesis_dangling),
                    soft_block_indent(&group(&format_inner)),
                    token(")")
                ]
            )
        }
    }
}

struct CommentsAroundText<'a> {
    text: &'static str,
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for CommentsAroundText<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if self.comments.is_empty() {
            token(self.text).fmt(f)
        } else {
            // There might be own line comments in trailing, but those are weird and we can kinda
            // ignore them
            // ```python
            // def f42(
            //     a,
            //     # leading comment (own line)
            //     / # first trailing comment (end-of-line)
            //     # trailing own line comment
            //     ,
            //     c,
            // ):
            // ```
            let (leading, trailing) = self.comments.split_at(
                self.comments
                    .partition_point(|comment| comment.line_position().is_own_line()),
            );
            write!(
                f,
                [
                    leading_comments(leading),
                    token(self.text),
                    trailing_comments(trailing)
                ]
            )
        }
    }
}

/// `/` and `*` in a function signature
///
/// ```text
/// def f(arg_a, /, arg_b, *, arg_c): pass
///            ^ ^  ^    ^ ^  ^ slash preceding end
///              ^  ^    ^ ^  ^ slash (a separator)
///                 ^    ^ ^  ^ slash following start
///                      ^ ^  ^ star preceding end
///                        ^  ^ star (a separator)
///                           ^ star following start
/// ```
#[derive(Debug)]
pub(crate) struct ParameterSeparator {
    /// The end of the last node or separator before this separator
    preceding_end: TextSize,
    /// The range of the separator itself
    separator: TextRange,
    /// The start of the first node or separator following this separator
    following_start: TextSize,
}

/// Finds a bare slash or star in the gap between parameter nodes. An unpacked
/// parameter includes its own stars, so `*args` and `**kwargs` are outside this gap.
pub(crate) fn find_parameter_separators(
    contents: &str,
    gap: TextRange,
) -> (Option<ParameterSeparator>, Option<ParameterSeparator>) {
    let mut slash = None;
    let mut star = None;
    for token in SimpleTokenizer::new(contents, gap).skip_trivia() {
        match token.kind {
            SimpleTokenKind::Slash => slash = Some(token.range),
            SimpleTokenKind::Star => star = Some(token.range),
            _ => {}
        }
    }
    let slash_separator = slash.map(|separator| ParameterSeparator {
        preceding_end: gap.start(),
        separator,
        following_start: star.map_or(gap.end(), TextRange::start),
    });
    let star_separator = star.map(|separator| ParameterSeparator {
        preceding_end: slash.map_or(gap.start(), TextRange::end),
        separator,
        following_start: gap.end(),
    });
    (slash_separator, star_separator)
}

/// Locates positional only parameters separator `/` or the keywords only parameters
/// separator `*` comments.
///
/// ```python
/// def test(
///     a,
///     # Positional only parameters after here
///     /, # trailing positional argument comment.
///     b,
/// ):
///     pass
/// ```
/// or
/// ```python
/// def f(
///     a="",
///     # Keyword only parameters only after here
///     *, # trailing keyword argument comment.
///     b="",
/// ):
///     pass
/// ```
/// or
/// ```python
/// def f(
///     a,
///     # positional only comment, leading
///     /,  # positional only comment, trailing
///     b,
///     # keyword only comment, leading
///     *, # keyword only comment, trailing
///     c,
/// ):
///     pass
/// ```
/// Notably, the following is possible:
/// ```python
/// def f32(
///     a,
///     # positional only comment, leading
///     /,  # positional only comment, trailing
///     # keyword only comment, leading
///     *, # keyword only comment, trailing
///     c,
/// ):
///     pass
/// ```
///
/// ## Background
///
/// ```text
/// def f(a1, a2): pass
///       ^^^^^^ parameters (args)
/// ```
/// Use a star to separate keyword only parameters:
/// ```text
/// def f(a1, a2, *, a3, a4): pass
///       ^^^^^^            parameters (args)
///                  ^^^^^^ keyword only parameters (kwargs)
/// ```
/// Use a slash to separate positional only parameters. Note that this changes the parameters left
/// of the slash while the star change the parameters right of it:
/// ```text
/// def f(a1, a2, /, a3, a4): pass
///       ^^^^^^            positional only parameters (posonlyargs)
///                  ^^^^^^ parameters (args)
/// ```
/// You can combine both:
/// ```text
/// def f(a1, a2, /, a3, a4, *, a5, a6): pass
///       ^^^^^^                       positional only parameters (posonlyargs)
///                  ^^^^^^            parameters (args)
///                             ^^^^^^ keyword only parameters (kwargs)
/// ```
/// They can all have defaults, meaning that the preceding node ends at the default instead of the
/// argument itself:
/// ```text
/// def f(a1=1, a2=2, /, a3=3, a4=4, *, a5=5, a6=6): pass
///          ^     ^        ^     ^        ^     ^ defaults
///       ^^^^^^^^^^                               positional only parameters (posonlyargs)
///                      ^^^^^^^^^^                parameters (args)
///                                     ^^^^^^^^^^ keyword only parameters (kwargs)
/// ```
/// An especially difficult case is having no regular parameters, so comments from both slash and
/// star will attach to either a2 or a3 and the next token is incorrect.
/// ```text
/// def f(a1, a2, /, *, a3, a4): pass
///       ^^^^^^               positional only parameters (posonlyargs)
///                     ^^^^^^ keyword only parameters (kwargs)
/// ```
pub(crate) fn assign_argument_separator_comment_placement(
    slash: Option<&ParameterSeparator>,
    star: Option<&ParameterSeparator>,
    comment_range: TextRange,
    text_position: CommentLinePosition,
) -> Option<ArgumentSeparatorCommentLocation> {
    if let Some(ParameterSeparator {
        preceding_end,
        separator: slash,
        following_start,
    }) = slash
    {
        // ```python
        // def f(
        //    # start too early
        //    a,  # not own line
        //    # this is the one
        //    /, # too late (handled later)
        //    b,
        // )
        // ```
        if comment_range.start() > *preceding_end
            && comment_range.start() < slash.start()
            && text_position.is_own_line()
        {
            return Some(ArgumentSeparatorCommentLocation::SlashLeading);
        }

        // ```python
        // def f(
        //    a,
        //    # too early (handled above)
        //    /, # this is the one
        //    # not end-of-line
        //    b,
        // )
        // ```
        if comment_range.start() > slash.end()
            && comment_range.start() < *following_start
            && text_position.is_end_of_line()
        {
            return Some(ArgumentSeparatorCommentLocation::SlashTrailing);
        }
    }

    if let Some(ParameterSeparator {
        preceding_end,
        separator: star,
        following_start,
    }) = star
    {
        // ```python
        // def f(
        //    # start too early
        //    a,  # not own line
        //    # this is the one
        //    *, # too late (handled later)
        //    b,
        // )
        // ```
        if comment_range.start() > *preceding_end
            && comment_range.start() < star.start()
            && text_position.is_own_line()
        {
            return Some(ArgumentSeparatorCommentLocation::StarLeading);
        }

        // ```python
        // def f(
        //    a,
        //    # too early (handled above)
        //    *, # this is the one
        //    # not end-of-line
        //    b,
        // )
        // ```
        if comment_range.start() > star.end()
            && comment_range.start() < *following_start
            && text_position.is_end_of_line()
        {
            return Some(ArgumentSeparatorCommentLocation::StarTrailing);
        }
    }
    None
}

/// ```python
/// def f(
///     a,
///     # before slash
///     /,  # after slash
///     b,
///     # before star
///     *, # after star
///     c,
/// ):
///     pass
/// ```
#[derive(Debug)]
pub(crate) enum ArgumentSeparatorCommentLocation {
    SlashLeading,
    SlashTrailing,
    StarLeading,
    StarTrailing,
}

fn has_trailing_comma(last_node: Option<AnyNodeRef>, ends_with_slash: bool, source: &str) -> bool {
    let Some(last_node) = last_node else {
        return false;
    };

    let mut tokens = SimpleTokenizer::starts_at(last_node.end(), source).skip_trivia();
    // `def a(b, c, /): ... `
    // The slash lacks its own node
    if ends_with_slash {
        let comma = tokens.next();
        assert_matches!(
            comma,
            Some(SimpleToken {
                kind: SimpleTokenKind::Comma,
                ..
            }),
            "The last positional only argument must be separated by a `,` from the positional only parameters separator `/`."
        );

        let slash = tokens.next();
        assert_matches!(
            slash,
            Some(SimpleToken {
                kind: SimpleTokenKind::Slash,
                ..
            }),
            "The positional argument separator must be present for a function that has positional only parameters."
        );
    }

    tokens
        .next()
        .expect("There must be a token after the argument list")
        .kind()
        == SimpleTokenKind::Comma
}
