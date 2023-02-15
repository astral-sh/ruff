//! Provides builders for comments and skipped token trivia.

use crate::format_element::tag::VerbatimKind;
use crate::prelude::*;
use crate::{
    comments::{CommentKind, CommentStyle},
    write, Argument, Arguments, CstFormatContext, FormatRefWithRule, GroupId, SourceComment,
    TextRange,
};
use ruff_rowan::{Language, SyntaxNode, SyntaxToken};
#[cfg(debug_assertions)]
use std::cell::Cell;

/// Formats the leading comments of `node`
pub const fn format_leading_comments<L: Language>(
    node: &SyntaxNode<L>,
) -> FormatLeadingComments<L> {
    FormatLeadingComments::Node(node)
}

/// Formats the leading comments of a node.
#[derive(Debug, Copy, Clone)]
pub enum FormatLeadingComments<'a, L: Language> {
    Node(&'a SyntaxNode<L>),
    Comments(&'a [SourceComment<L>]),
}

impl<Context> Format<Context> for FormatLeadingComments<'_, Context::Language>
where
    Context: CstFormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let leading_comments = match self {
            FormatLeadingComments::Node(node) => comments.leading_comments(node),
            FormatLeadingComments::Comments(comments) => comments,
        };

        for comment in leading_comments {
            let format_comment = FormatRefWithRule::new(comment, Context::CommentRule::default());
            write!(f, [format_comment])?;

            match comment.kind() {
                CommentKind::Block | CommentKind::InlineBlock => {
                    match comment.lines_after() {
                        0 => write!(f, [space()])?,
                        1 => {
                            if comment.lines_before() == 0 {
                                write!(f, [soft_line_break_or_space()])?;
                            } else {
                                write!(f, [hard_line_break()])?;
                            }
                        }
                        _ => write!(f, [empty_line()])?,
                    };
                }
                CommentKind::Line => match comment.lines_after() {
                    0 | 1 => write!(f, [hard_line_break()])?,
                    _ => write!(f, [empty_line()])?,
                },
            }

            comment.mark_formatted()
        }

        Ok(())
    }
}

/// Formats the trailing comments of `node`.
pub const fn format_trailing_comments<L: Language>(
    node: &SyntaxNode<L>,
) -> FormatTrailingComments<L> {
    FormatTrailingComments::Node(node)
}

/// Formats the trailing comments of `node`
#[derive(Debug, Clone, Copy)]
pub enum FormatTrailingComments<'a, L: Language> {
    Node(&'a SyntaxNode<L>),
    Comments(&'a [SourceComment<L>]),
}

impl<Context> Format<Context> for FormatTrailingComments<'_, Context::Language>
where
    Context: CstFormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let trailing_comments = match self {
            FormatTrailingComments::Node(node) => comments.trailing_comments(node),
            FormatTrailingComments::Comments(comments) => comments,
        };

        let mut total_lines_before = 0;

        for comment in trailing_comments {
            total_lines_before += comment.lines_before();

            let format_comment = FormatRefWithRule::new(comment, Context::CommentRule::default());

            // This allows comments at the end of nested structures:
            // {
            //   x: 1,
            //   y: 2
            //   // A comment
            // }
            // Those kinds of comments are almost always leading comments, but
            // here it doesn't go "outside" the block and turns it into a
            // trailing comment for `2`. We can simulate the above by checking
            // if this a comment on its own line; normal trailing comments are
            // always at the end of another expression.
            if total_lines_before > 0 {
                write!(
                    f,
                    [
                        line_suffix(&format_with(|f| {
                            match comment.lines_before() {
                                0 | 1 => write!(f, [hard_line_break()])?,
                                _ => write!(f, [empty_line()])?,
                            };

                            write!(f, [format_comment])
                        })),
                        expand_parent()
                    ]
                )?;
            } else {
                let content = format_with(|f| write!(f, [space(), format_comment]));
                if comment.kind().is_line() {
                    write!(f, [line_suffix(&content), expand_parent()])?;
                } else {
                    write!(f, [content])?;
                }
            }

            comment.mark_formatted();
        }

        Ok(())
    }
}

/// Formats the dangling comments of `node`.
pub const fn format_dangling_comments<L: Language>(
    node: &SyntaxNode<L>,
) -> FormatDanglingComments<L> {
    FormatDanglingComments::Node {
        node,
        indent: DanglingIndentMode::None,
    }
}

/// Formats the dangling trivia of `token`.
pub enum FormatDanglingComments<'a, L: Language> {
    Node {
        node: &'a SyntaxNode<L>,
        indent: DanglingIndentMode,
    },
    Comments {
        comments: &'a [SourceComment<L>],
        indent: DanglingIndentMode,
    },
}

#[derive(Copy, Clone, Debug)]
pub enum DanglingIndentMode {
    /// Writes every comment on its own line and indents them with a block indent.
    ///
    /// # Examples
    /// ```ignore
    /// [
    ///     /* comment */
    /// ]
    ///
    /// [
    ///     /* comment */
    ///     /* multiple */
    /// ]
    /// ```
    Block,

    /// Writes every comment on its own line and indents them with a soft line indent.
    /// Guarantees to write a line break if the last formatted comment is a [line](CommentKind::Line) comment.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// [/* comment */]
    ///
    /// [
    ///     /* comment */
    ///     /* other */
    /// ]
    ///
    /// [
    ///     // line
    /// ]
    /// ```
    Soft,

    /// Writes every comment on its own line.
    None,
}

impl<L: Language> FormatDanglingComments<'_, L> {
    /// Indents the comments with a [block](DanglingIndentMode::Block) indent.
    pub fn with_block_indent(self) -> Self {
        self.with_indent_mode(DanglingIndentMode::Block)
    }

    /// Indents the comments with a [soft block](DanglingIndentMode::Soft) indent.
    pub fn with_soft_block_indent(self) -> Self {
        self.with_indent_mode(DanglingIndentMode::Soft)
    }

    fn with_indent_mode(mut self, mode: DanglingIndentMode) -> Self {
        match &mut self {
            FormatDanglingComments::Node { indent, .. } => *indent = mode,
            FormatDanglingComments::Comments { indent, .. } => *indent = mode,
        }
        self
    }

    const fn indent(&self) -> DanglingIndentMode {
        match self {
            FormatDanglingComments::Node { indent, .. } => *indent,
            FormatDanglingComments::Comments { indent, .. } => *indent,
        }
    }
}

impl<Context> Format<Context> for FormatDanglingComments<'_, Context::Language>
where
    Context: CstFormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let dangling_comments = match self {
            FormatDanglingComments::Node { node, .. } => comments.dangling_comments(node),
            FormatDanglingComments::Comments { comments, .. } => *comments,
        };

        if dangling_comments.is_empty() {
            return Ok(());
        }

        let format_dangling_comments = format_with(|f| {
            // Write all comments up to the first skipped token trivia or the token
            let mut join = f.join_with(hard_line_break());

            for comment in dangling_comments {
                let format_comment =
                    FormatRefWithRule::new(comment, Context::CommentRule::default());
                join.entry(&format_comment);

                comment.mark_formatted();
            }

            join.finish()?;

            if matches!(self.indent(), DanglingIndentMode::Soft)
                && dangling_comments
                    .last()
                    .map_or(false, |comment| comment.kind().is_line())
            {
                write!(f, [hard_line_break()])?;
            }

            Ok(())
        });

        match self.indent() {
            DanglingIndentMode::Block => {
                write!(f, [block_indent(&format_dangling_comments)])
            }
            DanglingIndentMode::Soft => {
                write!(f, [group(&soft_block_indent(&format_dangling_comments))])
            }
            DanglingIndentMode::None => {
                write!(f, [format_dangling_comments])
            }
        }
    }
}

/// Formats a token without its skipped token trivia
///
/// ## Warning
/// It's your responsibility to format any skipped trivia.
pub const fn format_trimmed_token<L: Language>(token: &SyntaxToken<L>) -> FormatTrimmedToken<L> {
    FormatTrimmedToken { token }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct FormatTrimmedToken<'a, L: Language> {
    token: &'a SyntaxToken<L>,
}

impl<L: Language + 'static, C> Format<C> for FormatTrimmedToken<'_, L>
where
    C: CstFormatContext<Language = L>,
{
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        let trimmed_range = self.token.text_trimmed_range();
        syntax_token_text_slice(self.token, trimmed_range).fmt(f)
    }
}
/// Formats the skipped token trivia of a removed token and marks the token as tracked.
pub const fn format_removed<L>(token: &SyntaxToken<L>) -> FormatRemoved<L>
where
    L: Language,
{
    FormatRemoved { token }
}

/// Formats the trivia of a token that is present in the source text but should be omitted in the
/// formatted output.
pub struct FormatRemoved<'a, L>
where
    L: Language,
{
    token: &'a SyntaxToken<L>,
}

impl<C, L> Format<C> for FormatRemoved<'_, L>
where
    L: Language + 'static,
    C: CstFormatContext<Language = L>,
{
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        f.state_mut().track_token(self.token);

        write!(f, [format_skipped_token_trivia(self.token)])
    }
}

/// Print out a `token` from the original source with a different `content`.
///
/// This will print the skipped token trivia that belong to `token` to `content`;
/// `token` is then marked as consumed by the formatter.
pub fn format_replaced<'a, 'content, L, Context>(
    token: &'a SyntaxToken<L>,
    content: &'content impl Format<Context>,
) -> FormatReplaced<'a, 'content, L, Context>
where
    L: Language,
{
    FormatReplaced {
        token,
        content: Argument::new(content),
    }
}

/// Formats a token's skipped token trivia but uses the provided content instead
/// of the token in the formatted output.
#[derive(Copy, Clone)]
pub struct FormatReplaced<'a, 'content, L, C>
where
    L: Language,
{
    token: &'a SyntaxToken<L>,
    content: Argument<'content, C>,
}

impl<L, C> Format<C> for FormatReplaced<'_, '_, L, C>
where
    L: Language + 'static,
    C: CstFormatContext<Language = L>,
{
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        f.state_mut().track_token(self.token);

        write!(f, [format_skipped_token_trivia(self.token)])?;

        f.write_fmt(Arguments::from(&self.content))
    }
}

/// Formats the given token only if the group does break and otherwise retains the token's skipped token trivia.
pub fn format_only_if_breaks<'a, 'content, L, Content, Context>(
    token: &'a SyntaxToken<L>,
    content: &'content Content,
) -> FormatOnlyIfBreaks<'a, 'content, L, Context>
where
    L: Language,
    Content: Format<Context>,
{
    FormatOnlyIfBreaks {
        token,
        content: Argument::new(content),
        group_id: None,
    }
}

/// Formats a token with its skipped token trivia that only gets printed if its enclosing
/// group does break but otherwise gets omitted from the formatted output.
pub struct FormatOnlyIfBreaks<'a, 'content, L, C>
where
    L: Language,
{
    token: &'a SyntaxToken<L>,
    content: Argument<'content, C>,
    group_id: Option<GroupId>,
}

impl<'a, 'content, L, C> FormatOnlyIfBreaks<'a, 'content, L, C>
where
    L: Language,
{
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.group_id = group_id;
        self
    }
}

impl<L, C> Format<C> for FormatOnlyIfBreaks<'_, '_, L, C>
where
    L: Language + 'static,
    C: CstFormatContext<Language = L>,
{
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        write!(
            f,
            [if_group_breaks(&Arguments::from(&self.content)).with_group_id(self.group_id),]
        )?;

        if f.comments().has_skipped(self.token) {
            // Print the trivia otherwise
            write!(
                f,
                [
                    if_group_fits_on_line(&format_skipped_token_trivia(self.token))
                        .with_group_id(self.group_id)
                ]
            )?;
        }

        Ok(())
    }
}

/// Formats the skipped token trivia of `token`.
pub const fn format_skipped_token_trivia<L: Language>(
    token: &SyntaxToken<L>,
) -> FormatSkippedTokenTrivia<L> {
    FormatSkippedTokenTrivia { token }
}

/// Formats the skipped token trivia of `token`.
pub struct FormatSkippedTokenTrivia<'a, L: Language> {
    token: &'a SyntaxToken<L>,
}

impl<L: Language> FormatSkippedTokenTrivia<'_, L> {
    #[cold]
    fn fmt_skipped<Context>(&self, f: &mut Formatter<Context>) -> FormatResult<()>
    where
        Context: CstFormatContext<Language = L>,
    {
        // Lines/spaces before the next token/comment
        let (mut lines, mut spaces) = match self.token.prev_token() {
            Some(token) => {
                let mut lines = 0u32;
                let mut spaces = 0u32;
                for piece in token.trailing_trivia().pieces().rev() {
                    if piece.is_whitespace() {
                        spaces += 1;
                    } else if piece.is_newline() {
                        spaces = 0;
                        lines += 1;
                    } else {
                        break;
                    }
                }

                (lines, spaces)
            }
            None => (0, 0),
        };

        // The comments between the last skipped token trivia and the token
        let mut dangling_comments = Vec::new();
        let mut skipped_range: Option<TextRange> = None;

        // Iterate over the remaining pieces to find the full range from the first to the last skipped token trivia.
        // Extract the comments between the last skipped token trivia and the token.
        for piece in self.token.leading_trivia().pieces() {
            if piece.is_whitespace() {
                spaces += 1;
                continue;
            }

            if piece.is_newline() {
                lines += 1;
                spaces = 0;
            } else if let Some(comment) = piece.as_comments() {
                let source_comment = SourceComment {
                    kind: Context::Style::get_comment_kind(&comment),
                    lines_before: lines,
                    lines_after: 0,
                    piece: comment,
                    #[cfg(debug_assertions)]
                    formatted: Cell::new(true),
                };

                dangling_comments.push(source_comment);

                lines = 0;
                spaces = 0;
            } else if piece.is_skipped() {
                skipped_range = Some(match skipped_range {
                    Some(range) => range.cover(piece.text_range()),
                    None => {
                        if dangling_comments.is_empty() {
                            match lines {
                                0 if spaces == 0 => {
                                    // Token had no space to previous token nor any preceding comment. Keep it that way
                                }
                                0 => write!(f, [space()])?,
                                _ => write!(f, [hard_line_break()])?,
                            };
                        } else {
                            match lines {
                                0 => write!(f, [space()])?,
                                1 => write!(f, [hard_line_break()])?,
                                _ => write!(f, [empty_line()])?,
                            };
                        }

                        piece.text_range()
                    }
                });

                lines = 0;
                spaces = 0;
                dangling_comments.clear();
            }
        }

        let skipped_range =
            skipped_range.unwrap_or_else(|| TextRange::empty(self.token.text_range().start()));

        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            VerbatimKind::Verbatim {
                length: skipped_range.len(),
            },
        )))?;
        write!(f, [syntax_token_text_slice(self.token, skipped_range)])?;
        f.write_element(FormatElement::Tag(Tag::EndVerbatim))?;

        // Write whitespace separator between skipped/last comment and token
        if dangling_comments.is_empty() {
            match lines {
                0 if spaces == 0 => {
                    // Don't write a space if there was non in the source document
                    Ok(())
                }
                0 => write!(f, [space()]),
                _ => write!(f, [hard_line_break()]),
            }
        } else {
            match dangling_comments.first().unwrap().lines_before {
                0 => write!(f, [space()])?,
                1 => write!(f, [hard_line_break()])?,
                _ => write!(f, [empty_line()])?,
            }

            write!(
                f,
                [FormatDanglingComments::Comments {
                    comments: &dangling_comments,
                    indent: DanglingIndentMode::None
                }]
            )?;

            match lines {
                0 => write!(f, [space()]),
                _ => write!(f, [hard_line_break()]),
            }
        }
    }
}

impl<Context> Format<Context> for FormatSkippedTokenTrivia<'_, Context::Language>
where
    Context: CstFormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        if f.comments().has_skipped(self.token) {
            self.fmt_skipped(f)
        } else {
            Ok(())
        }
    }
}
