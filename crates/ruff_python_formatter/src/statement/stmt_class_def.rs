use ruff_formatter::write;
use ruff_python_ast::{Ranged, StmtClassDef};
use ruff_python_trivia::{lines_after, skip_trailing_trivia};

use crate::comments::{leading_comments, trailing_comments};
use crate::prelude::*;
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
            type_params: _,
            decorator_list,
        } = item;

        let comments = f.context().comments().clone();

        let dangling_comments = comments.dangling_comments(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        if let Some(last_decorator) = decorator_list.last() {
            f.join_with(hard_line_break())
                .entries(decorator_list.iter().formatted())
                .finish()?;

            if leading_definition_comments.is_empty() {
                write!(f, [hard_line_break()])?;
            } else {
                // Write any leading definition comments (between last decorator and the header)
                // while maintaining the right amount of empty lines between the comment
                // and the last decorator.
                let decorator_end =
                    skip_trailing_trivia(last_decorator.end(), f.context().source());

                let leading_line = if lines_after(decorator_end, f.context().source()) <= 1 {
                    hard_line_break()
                } else {
                    empty_line()
                };

                write!(
                    f,
                    [leading_line, leading_comments(leading_definition_comments)]
                )?;
            }
        }

        write!(f, [text("class"), space(), name.format()])?;

        if let Some(arguments) = arguments.as_deref() {
            // Drop empty parentheses, e.g., in:
            // ```python
            // class A():
            //     ...
            // ```
            //
            // However, preserve any dangling end-of-line comments, e.g., in:
            // ```python
            // class A(  # comment
            // ):
            //     ...
            //
            // If the arguments contain any dangling own-line comments, we retain the parentheses,
            // e.g., in:
            // ```python
            // class A(  # comment
            //     # comment
            // ):
            //     ...
            // ```
            if arguments.args.is_empty()
                && arguments.keywords.is_empty()
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

        write!(
            f,
            [
                text(":"),
                trailing_comments(trailing_definition_comments),
                block_indent(&body.format().with_options(SuiteKind::Class))
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtClassDef,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // handled in fmt_fields
        Ok(())
    }
}
