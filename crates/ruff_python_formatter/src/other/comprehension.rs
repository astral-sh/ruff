use crate::comments::{leading_comments, trailing_comments, SourceComment};
use crate::prelude::*;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::{Comprehension, Ranged};

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, item: &Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        let Comprehension {
            range: _,
            target,
            iter,
            ifs,
            is_async,
        } = item;

        let comments = f.context().comments().clone();
        let leading_item_comments = comments.leading_comments(item);

        // TODO: why is this needed?
        if let Some(leading_item_comment) = leading_item_comments.first() {
            if leading_item_comment.line_position().is_own_line() {
                hard_line_break().fmt(f)?;
            }
        }
        leading_comments(leading_item_comments).fmt(f)?;

        if *is_async {
            write!(f, [text("async"), space()])?;
        }

        let dangling_item_comments = comments.dangling_comments(item);

        let (before_target_comments, before_in_comments) = dangling_item_comments.split_at(
            dangling_item_comments
                .partition_point(|comment| comment.slice().end() < target.range().start()),
        );

        let trailing_in_comments = comments.dangling_comments(iter);
        write!(
            f,
            [
                text("for"),
                trailing_comments(before_target_comments),
                group(&self::format_args!(
                    soft_line_break_or_space(),
                    &target.format(),
                    soft_line_break_or_space(),
                    leading_comments(before_in_comments),
                    text("in"),
                    trailing_comments(trailing_in_comments),
                    soft_line_break_or_space(),
                    iter.format(),
                )),
            ]
        )?;
        if !ifs.is_empty() {
            let joined = format_with(|f| {
                let mut joiner = f.join_with(soft_line_break_or_space());
                for if_ in ifs {
                    let dangling_if_comments = comments.dangling_comments(if_);

                    let (own_line_if_comments, end_of_line_if_comments) = dangling_if_comments
                        .split_at(
                            dangling_if_comments
                                .partition_point(|comment| comment.line_position().is_own_line()),
                        );
                    joiner.entry(&group(&self::format_args!(
                        leading_comments(own_line_if_comments),
                        text("if"),
                        trailing_comments(end_of_line_if_comments),
                        soft_line_break_or_space(),
                        if_.format(),
                    )));
                }
                joiner.finish()
            });

            write!(f, [soft_line_break_or_space(), group(&joined)])?;
        }
        Ok(())
    }

    fn fmt_leading_comments(
        &self,
        _item: &Comprehension,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct FormatLeadingCommentsSpacing<'a> {
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatLeadingCommentsSpacing<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(first) = self.comments.first() {
            if first.line_position().is_own_line() {
                // Insert a newline after the colon so the comment ends up on its own line
                hard_line_break().fmt(f)?;
            } else {
                // Insert the two spaces between the colon and the end-of-line comment after the colon
                write!(f, [space(), space()])?;
            }
        }
        Ok(())
    }
}
