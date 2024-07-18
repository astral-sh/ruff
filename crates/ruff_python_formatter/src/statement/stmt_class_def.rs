use ruff_formatter::write;
use ruff_python_ast::{Decorator, NodeKind, StmtClassDef};
use ruff_python_trivia::lines_after_ignoring_end_of_line_trivia;
use ruff_text_size::Ranged;

use crate::comments::format::{
    empty_lines_after_leading_comments, empty_lines_before_trailing_comments,
};
use crate::comments::{leading_comments, trailing_comments, SourceComment};
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::statement::suite::SuiteKind;

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

        let dangling_comments = comments.dangling(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        // If the class contains leading comments, insert newlines before them.
        // For example, given:
        // ```python
        // # comment
        //
        // class Class:
        //     ...
        // ```
        //
        // At the top-level in a non-stub file, reformat as:
        // ```python
        // # comment
        //
        //
        // class Class:
        //     ...
        // ```
        // Note that this is only really relevant for the specific case in which there's a single
        // newline between the comment and the node, but we _require_ two newlines. If there are
        // _no_ newlines between the comment and the node, we don't insert _any_ newlines; if there
        // are more than two, then `leading_comments` will preserve the correct number of newlines.
        empty_lines_after_leading_comments(comments.leading(item)).fmt(f)?;

        write!(
            f,
            [
                FormatDecorators {
                    decorators: decorator_list,
                    leading_definition_comments,
                },
                clause_header(
                    ClauseHeader::Class(item),
                    trailing_definition_comments,
                    &format_with(|f| {
                        write!(f, [token("class"), space(), name.format()])?;

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
                                    .dangling(arguments)
                                    .iter()
                                    .all(|comment| comment.line_position().is_end_of_line())
                            {
                                let dangling = comments.dangling(arguments);
                                write!(f, [trailing_comments(dangling)])?;
                            } else {
                                write!(f, [arguments.format()])?;
                            }
                        }

                        Ok(())
                    }),
                ),
                clause_body(body, SuiteKind::Class, trailing_definition_comments),
            ]
        )?;

        // If the class contains trailing comments, insert newlines before them.
        // For example, given:
        // ```python
        // class Class:
        //     ...
        // # comment
        // ```
        //
        // At the top-level in a non-stub file, reformat as:
        // ```python
        // class Class:
        //     ...
        //
        //
        // # comment
        // ```
        empty_lines_before_trailing_comments(comments.trailing(item), NodeKind::StmtClassDef)
            .fmt(f)?;

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
                let leading_line = if lines_after_ignoring_end_of_line_trivia(
                    last_decorator.end(),
                    f.context().source(),
                ) <= 1
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
