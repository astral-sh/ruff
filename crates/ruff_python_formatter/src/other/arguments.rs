use std::usize;

use rustpython_parser::ast::{Arguments, Ranged};

use ruff_formatter::{format_args, write};
use ruff_python_ast::node::{AnyNodeRef, AstNode};

use crate::comments::{
    dangling_comments, leading_comments, leading_node_comments, trailing_comments,
    CommentLinePosition, SourceComment,
};
use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{first_non_trivia_token, SimpleTokenizer, Token, TokenKind};
use crate::FormatNodeRule;
use ruff_text_size::{TextRange, TextSize};

#[derive(Default)]
pub struct FormatArguments;

impl FormatNodeRule<Arguments> for FormatArguments {
    fn fmt_fields(&self, item: &Arguments, f: &mut PyFormatter) -> FormatResult<()> {
        let Arguments {
            range: _,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = item;

        let saved_level = f.context().node_level();
        f.context_mut().set_node_level(NodeLevel::Expression);

        let comments = f.context().comments().clone();
        let dangling = comments.dangling_comments(item);
        let (slash, star) = find_argument_separators(f.context().contents(), item);

        let format_inner = format_with(|f: &mut PyFormatter| {
            let separator = format_with(|f| write!(f, [text(","), soft_line_break_or_space()]));
            let mut joiner = f.join_with(separator);
            let mut last_node: Option<AnyNodeRef> = None;

            for arg_with_default in posonlyargs {
                joiner.entry(&arg_with_default.format());

                last_node = Some(arg_with_default.into());
            }

            let slash_comments_end = if posonlyargs.is_empty() {
                0
            } else {
                let slash_comments_end = dangling.partition_point(|comment| {
                    let assignment = assign_argument_separator_comment_placement(
                        slash.as_ref(),
                        star.as_ref(),
                        comment.slice().range(),
                        comment.line_position(),
                    )
                    .expect("Unexpected dangling comment type in function arguments");
                    matches!(
                        assignment,
                        ArgumentSeparatorCommentLocation::SlashLeading
                            | ArgumentSeparatorCommentLocation::SlashTrailing
                    )
                });
                joiner.entry(&CommentsAroundText {
                    text: "/",
                    comments: &dangling[..slash_comments_end],
                });
                slash_comments_end
            };

            for arg_with_default in args {
                joiner.entry(&arg_with_default.format());

                last_node = Some(arg_with_default.into());
            }

            // kw only args need either a `*args` ahead of them capturing all var args or a `*`
            // pseudo-argument capturing all fields. We can also have `*args` without any kwargs
            // afterwards.
            if let Some(vararg) = vararg {
                joiner.entry(&format_args![
                    leading_node_comments(vararg.as_ref()),
                    text("*"),
                    vararg.format()
                ]);
                last_node = Some(vararg.as_any_node_ref());
            } else if !kwonlyargs.is_empty() {
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
                    comments: &dangling[slash_comments_end..],
                });
            }

            for arg_with_default in kwonlyargs {
                joiner.entry(&arg_with_default.format());

                last_node = Some(arg_with_default.into());
            }

            if let Some(kwarg) = kwarg {
                joiner.entry(&format_args![
                    leading_node_comments(kwarg.as_ref()),
                    text("**"),
                    kwarg.format()
                ]);
                last_node = Some(kwarg.as_any_node_ref());
            }

            joiner.finish()?;

            write!(f, [if_group_breaks(&text(","))])?;

            // Expand the group if the source has a trailing *magic* comma.
            if let Some(last_node) = last_node {
                let ends_with_pos_only_argument_separator = !posonlyargs.is_empty()
                    && args.is_empty()
                    && vararg.is_none()
                    && kwonlyargs.is_empty()
                    && kwarg.is_none();

                let maybe_comma_token = if ends_with_pos_only_argument_separator {
                    // `def a(b, c, /): ... `
                    let mut tokens =
                        SimpleTokenizer::starts_at(last_node.end(), f.context().contents())
                            .skip_trivia();

                    let comma = tokens.next();
                    assert!(matches!(comma, Some(Token { kind: TokenKind::Comma, .. })), "The last positional only argument must be separated by a `,` from the positional only arguments separator `/` but found '{comma:?}'.");

                    let slash = tokens.next();
                    assert!(matches!(slash, Some(Token { kind: TokenKind::Slash, .. })), "The positional argument separator must be present for a function that has positional only arguments but found '{slash:?}'.");

                    tokens.next()
                } else {
                    first_non_trivia_token(last_node.end(), f.context().contents())
                };

                if maybe_comma_token.map_or(false, |token| token.kind() == TokenKind::Comma) {
                    write!(f, [hard_line_break()])?;
                }
            }

            Ok(())
        });

        let num_arguments = posonlyargs.len()
            + args.len()
            + usize::from(vararg.is_some())
            + kwonlyargs.len()
            + usize::from(kwarg.is_some());

        if num_arguments == 0 {
            // No arguments, format any dangling comments between `()`
            write!(
                f,
                [
                    text("("),
                    block_indent(&dangling_comments(dangling)),
                    text(")")
                ]
            )?;
        } else {
            write!(
                f,
                [group(&format_args!(
                    text("("),
                    soft_block_indent(&group(&format_inner)),
                    text(")")
                ))]
            )?;
        }

        f.context_mut().set_node_level(saved_level);

        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &Arguments, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

struct CommentsAroundText<'a> {
    text: &'static str,
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for CommentsAroundText<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        if self.comments.is_empty() {
            text(self.text).fmt(f)
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
                    text(self.text),
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
pub(crate) struct ArgumentSeparator {
    /// The end of the last node or separator before this separator
    pub(crate) preceding_end: TextSize,
    /// The range of the separator itself
    pub(crate) separator: TextRange,
    /// The start of the first node or separator following this separator
    pub(crate) following_start: TextSize,
}

/// Finds slash and star in `f(a, /, b, *, c)`
///
/// Returns slash and star
pub(crate) fn find_argument_separators(
    contents: &str,
    arguments: &Arguments,
) -> (Option<ArgumentSeparator>, Option<ArgumentSeparator>) {
    // We only compute preceding_end and token location here since following_start depends on the
    // star location, but the star location depends on slash's position
    let slash = if let Some(preceding_end) = arguments.posonlyargs.last().map(Ranged::end) {
        // ```text
        // def f(a1=1, a2=2, /, a3, a4): pass
        //                 ^^^^^^^^^^^ the range (defaults)
        // def f(a1, a2, /, a3, a4): pass
        //             ^^^^^^^^^^^^ the range (no default)
        // ```
        let range = TextRange::new(preceding_end, arguments.end());
        let mut tokens = SimpleTokenizer::new(contents, range).skip_trivia();

        let comma = tokens
            .next()
            .expect("The function definition can't end here");
        debug_assert!(comma.kind() == TokenKind::Comma, "{comma:?}");
        let slash = tokens
            .next()
            .expect("The function definition can't end here");
        debug_assert!(slash.kind() == TokenKind::Slash, "{slash:?}");

        Some((preceding_end, slash.range))
    } else {
        None
    };

    // If we have a vararg we have a node that the comments attach to
    let star = if arguments.vararg.is_some() {
        // When the vararg is present the comments attach there and we don't need to do manual
        // formatting
        None
    } else if let Some(first_keyword_argument) = arguments.kwonlyargs.first() {
        // Check in that order:
        // * `f(a, /, b, *, c)` and `f(a=1, /, b=2, *, c)`
        // * `f(a, /, *, b)`
        // * `f(*, b)` (else branch)
        let after_arguments = arguments
            .args
            .last()
            .map(|arg| arg.range.end())
            .or(slash.map(|(_, slash)| slash.end()));
        if let Some(preceding_end) = after_arguments {
            let range = TextRange::new(preceding_end, arguments.end());
            let mut tokens = SimpleTokenizer::new(contents, range).skip_trivia();

            let comma = tokens
                .next()
                .expect("The function definition can't end here");
            debug_assert!(comma.kind() == TokenKind::Comma, "{comma:?}");
            let star = tokens
                .next()
                .expect("The function definition can't end here");
            debug_assert!(star.kind() == TokenKind::Star, "{star:?}");

            Some(ArgumentSeparator {
                preceding_end,
                separator: star.range,
                following_start: first_keyword_argument.start(),
            })
        } else {
            let mut tokens = SimpleTokenizer::new(contents, arguments.range).skip_trivia();

            let lparen = tokens
                .next()
                .expect("The function definition can't end here");
            debug_assert!(lparen.kind() == TokenKind::LParen, "{lparen:?}");
            let star = tokens
                .next()
                .expect("The function definition can't end here");
            debug_assert!(star.kind() == TokenKind::Star, "{star:?}");
            Some(ArgumentSeparator {
                preceding_end: arguments.range.start(),
                separator: star.range,
                following_start: first_keyword_argument.start(),
            })
        }
    } else {
        None
    };

    // Now that we have star, compute how long slash trailing comments can go
    // Check in that order:
    // * `f(a, /, b)`
    // * `f(a, /, *b)`
    // * `f(a, /, *, b)`
    // * `f(a, /)`
    let slash_following_start = arguments
        .args
        .first()
        .map(Ranged::start)
        .or(arguments.vararg.as_ref().map(|first| first.start()))
        .or(star.as_ref().map(|star| star.separator.start()))
        .unwrap_or(arguments.end());
    let slash = slash.map(|(preceding_end, slash)| ArgumentSeparator {
        preceding_end,
        separator: slash,
        following_start: slash_following_start,
    });

    (slash, star)
}

/// Locates positional only arguments separator `/` or the keywords only arguments
/// separator `*` comments.
///
/// ```python
/// def test(
///     a,
///     # Positional only arguments after here
///     /, # trailing positional argument comment.
///     b,
/// ):
///     pass
/// ```
/// or
/// ```python
/// def f(
///     a="",
///     # Keyword only arguments only after here
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
///       ^^^^^^ arguments (args)
/// ```
/// Use a star to separate keyword only arguments:
/// ```text
/// def f(a1, a2, *, a3, a4): pass
///       ^^^^^^            arguments (args)
///                  ^^^^^^ keyword only arguments (kwargs)
/// ```
/// Use a slash to separate positional only arguments. Note that this changes the arguments left
/// of the slash while the star change the arguments right of it:
/// ```text
/// def f(a1, a2, /, a3, a4): pass
///       ^^^^^^            positional only arguments (posonlyargs)
///                  ^^^^^^ arguments (args)
/// ```
/// You can combine both:
/// ```text
/// def f(a1, a2, /, a3, a4, *, a5, a6): pass
///       ^^^^^^                       positional only arguments (posonlyargs)
///                  ^^^^^^            arguments (args)
///                             ^^^^^^ keyword only arguments (kwargs)
/// ```
/// They can all have defaults, meaning that the preceding node ends at the default instead of the
/// argument itself:
/// ```text
/// def f(a1=1, a2=2, /, a3=3, a4=4, *, a5=5, a6=6): pass
///          ^     ^        ^     ^        ^     ^ defaults
///       ^^^^^^^^^^                               positional only arguments (posonlyargs)
///                      ^^^^^^^^^^                arguments (args)
///                                     ^^^^^^^^^^ keyword only arguments (kwargs)
/// ```
/// An especially difficult case is having no regular arguments, so comments from both slash and
/// star will attach to either a2 or a3 and the next token is incorrect.
/// ```text
/// def f(a1, a2, /, *, a3, a4): pass
///       ^^^^^^               positional only arguments (posonlyargs)
///                     ^^^^^^ keyword only arguments (kwargs)
/// ```
pub(crate) fn assign_argument_separator_comment_placement(
    slash: Option<&ArgumentSeparator>,
    star: Option<&ArgumentSeparator>,
    comment_range: TextRange,
    text_position: CommentLinePosition,
) -> Option<ArgumentSeparatorCommentLocation> {
    if let Some(ArgumentSeparator {
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

    if let Some(ArgumentSeparator {
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
