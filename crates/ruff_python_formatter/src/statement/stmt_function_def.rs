use ruff_formatter::write;
use ruff_python_ast::{Ranged, StmtFunctionDef};
use ruff_python_trivia::lines_after_ignoring_trivia;

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::{optional_parentheses, Parentheses};
use crate::prelude::*;
use crate::statement::suite::SuiteKind;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtFunctionDef;

impl FormatNodeRule<StmtFunctionDef> for FormatStmtFunctionDef {
    fn fmt_fields(&self, item: &StmtFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let dangling_comments = comments.dangling_comments(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        if let Some(last_decorator) = item.decorator_list.last() {
            f.join_with(hard_line_break())
                .entries(item.decorator_list.iter().formatted())
                .finish()?;

            if leading_definition_comments.is_empty() {
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
                    [leading_line, leading_comments(leading_definition_comments)]
                )?;
            }
        }

        if item.is_async {
            write!(f, [text("async"), space()])?;
        }

        write!(f, [text("def"), space(), item.name.format()])?;

        if let Some(type_params) = item.type_params.as_ref() {
            write!(f, [type_params.format()])?;
        }

        write!(f, [item.parameters.format()])?;

        if let Some(return_annotation) = item.returns.as_ref() {
            write!(f, [space(), text("->"), space()])?;
            if return_annotation.is_tuple_expr() {
                write!(
                    f,
                    [return_annotation.format().with_options(Parentheses::Never)]
                )?;
            } else {
                write!(
                    f,
                    [optional_parentheses(
                        &return_annotation.format().with_options(Parentheses::Never),
                    )]
                )?;
            }
        }

        write!(
            f,
            [
                text(":"),
                trailing_comments(trailing_definition_comments),
                block_indent(&item.body.format().with_options(SuiteKind::Function))
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtFunctionDef,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
