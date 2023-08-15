use ruff_formatter::write;
use ruff_python_ast::{Decorator, Ranged, StmtClassDef};
use ruff_python_trivia::lines_after_ignoring_trivia;

use crate::comments::{leading_comments, trailing_comments, SourceComment, SuppressionKind};
use crate::prelude::*;
use crate::statement::suite::SuiteKind;
use crate::verbatim::SuppressedClauseHeader;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtClassDef;

impl FormatNodeRule<StmtClassDef> for FormatStmtClassDef {
    fn fmt_fields(&self, item: &StmtClassDef, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtClassDef {
            range: _,
            name,
            arguments,
            body,
            type_params,
            decorator_list,
        } = item;

        let comments = f.context().comments().clone();

        let dangling_comments = comments.dangling_comments(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        FormatDecorators {
            decorators: decorator_list,
            leading_definition_comments,
        }
        .fmt(f)?;

        if SuppressionKind::has_skip_comment(trailing_definition_comments, f.context().source()) {
            SuppressedClauseHeader::Class(item).fmt(f)?;
        } else {
            write!(f, [text("class"), space(), name.format()])?;

            if let Some(type_params) = type_params.as_deref() {
                write!(f, [type_params.format()])?;
            }

            if let Some(arguments) = arguments.as_deref() {
                // Drop empty the arguments node entirely (i.e., remove the parentheses) if it is empty,
                // e.g., given:
                // ```python
                // class A():
                //     ...
                // ```
                //
                // Format as:
                // ```python
                // class A:
                //     ...
                // ```
                //
                // However, preserve any dangling end-of-line comments, e.g., given:
                // ```python
                // class A(  # comment
                // ):
                //     ...
                //
                // Format as:
                // ```python
                // class A:  # comment
                //     ...
                // ```
                //
                // However, the arguments contain any dangling own-line comments, we retain the
                // parentheses, e.g., given:
                // ```python
                // class A(  # comment
                //     # comment
                // ):
                //     ...
                // ```
                //
                // Format as:
                // ```python
                // class A(  # comment
                //     # comment
                // ):
                //     ...
                // ```
                if arguments.is_empty()
                    && comments
                        .dangling_comments(arguments)
                        .iter()
                        .all(|comment| comment.line_position().is_end_of_line())
                {
                    let dangling = comments.dangling_comments(arguments);
                    write!(f, [trailing_comments(dangling)])?;
                } else {
                    write!(f, [arguments.format()])?;
                }
            }

            write!(f, [text(":"),])?;
        }

        write!(
            f,
            [
                trailing_comments(trailing_definition_comments),
                block_indent(&body.format().with_options(SuiteKind::Class))
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // handled in fmt_fields
        Ok(())
    }
}

pub(super) struct FormatDecorators<'a> {
    pub(super) decorators: &'a [Decorator],
    pub(super) leading_definition_comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatDecorators<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        if let Some(last_decorator) = self.decorators.last() {
            f.join_with(hard_line_break())
                .entries(self.decorators.iter().formatted())
                .finish()?;

            if self.leading_definition_comments.is_empty() {
                write!(f, [hard_line_break()])?;
            } else {
                // Write any leading definition comments (between last decorator and the header)
                // while maintaining the right amount of empty lines between the comment
                // and the last decorator.
                let leading_line =
                    if lines_after_ignoring_trivia(last_decorator.end(), f.context().source()) <= 1
                    {
                        hard_line_break()
                    } else {
                        empty_line()
                    };

                write!(
                    f,
                    [
                        leading_line,
                        leading_comments(self.leading_definition_comments)
                    ]
                )?;
            }
        }

        Ok(())
    }
}
