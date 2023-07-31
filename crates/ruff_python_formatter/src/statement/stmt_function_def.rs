use ruff_python_ast::{Ranged, StmtFunctionDef};

use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::function::AnyFunctionDefinition;
use ruff_python_trivia::{lines_after, skip_trailing_trivia};

use crate::comments::{leading_comments, trailing_comments};

use crate::expression::parentheses::{optional_parentheses, Parentheses};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtFunctionDef;

impl FormatNodeRule<StmtFunctionDef> for FormatStmtFunctionDef {
    fn fmt_fields(&self, item: &StmtFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        AnyFunctionDefinition::from(item).format().fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtFunctionDef,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled by `AnyFunctionDef`
        Ok(())
    }
}

#[derive(Default)]
pub struct FormatAnyFunctionDef;

impl FormatRule<AnyFunctionDefinition<'_>, PyFormatContext<'_>> for FormatAnyFunctionDef {
    fn fmt(
        &self,
        item: &AnyFunctionDefinition<'_>,
        f: &mut Formatter<PyFormatContext<'_>>,
    ) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let dangling_comments = comments.dangling_comments(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_function_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        if let Some(last_decorator) = item.decorators().last() {
            f.join_with(hard_line_break())
                .entries(item.decorators().iter().formatted())
                .finish()?;

            if leading_function_definition_comments.is_empty() {
                write!(f, [hard_line_break()])?;
            } else {
                // Write any leading function comments (between last decorator and function header)
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
                    [
                        leading_line,
                        leading_comments(leading_function_definition_comments)
                    ]
                )?;
            }
        }

        if item.is_async() {
            write!(f, [text("async"), space()])?;
        }

        let name = item.name();

        write!(
            f,
            [
                text("def"),
                space(),
                name.format(),
                item.arguments().format(),
            ]
        )?;

        if let Some(return_annotation) = item.returns() {
            write!(
                f,
                [
                    space(),
                    text("->"),
                    space(),
                    optional_parentheses(
                        &return_annotation.format().with_options(Parentheses::Never)
                    )
                ]
            )?;
        }

        write!(
            f,
            [
                text(":"),
                trailing_comments(trailing_definition_comments),
                block_indent(&item.body().format())
            ]
        )
    }
}

impl<'def, 'ast> AsFormat<PyFormatContext<'ast>> for AnyFunctionDefinition<'def> {
    type Format<'a> = FormatRefWithRule<
        'a,
        AnyFunctionDefinition<'def>,
        FormatAnyFunctionDef,
        PyFormatContext<'ast>,
    > where Self: 'a;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatAnyFunctionDef)
    }
}

impl<'def, 'ast> IntoFormat<PyFormatContext<'ast>> for AnyFunctionDefinition<'def> {
    type Format = FormatOwnedWithRule<
        AnyFunctionDefinition<'def>,
        FormatAnyFunctionDef,
        PyFormatContext<'ast>,
    >;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatAnyFunctionDef)
    }
}
