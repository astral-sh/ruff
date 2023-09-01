use ruff_formatter::write;
use ruff_python_ast::WithItem;

use crate::comments::SourceComment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{parenthesized, Parentheses, Parenthesize};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatWithItem;

impl FormatNodeRule<WithItem> for FormatWithItem {
    fn fmt_fields(&self, item: &WithItem, f: &mut PyFormatter) -> FormatResult<()> {
        let WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = item;

        let comments = f.context().comments().clone();
        let trailing_as_comments = comments.dangling(item);

        write!(
            f,
            [maybe_parenthesize_expression(
                context_expr,
                item,
                Parenthesize::IfRequired
            )]
        )?;

        if let Some(optional_vars) = optional_vars {
            write!(f, [space(), token("as"), space()])?;

            if trailing_as_comments.is_empty() {
                write!(f, [optional_vars.format()])?;
            } else {
                write!(
                    f,
                    [parenthesized(
                        "(",
                        &optional_vars.format().with_options(Parentheses::Never),
                        ")",
                    )
                    .with_dangling_comments(trailing_as_comments)]
                )?;
            }
        }

        Ok(())
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}
