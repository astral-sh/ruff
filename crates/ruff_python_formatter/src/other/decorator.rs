use ruff_formatter::write;
use ruff_python_ast::Decorator;

use crate::comments::SourceComment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatDecorator;

impl FormatNodeRule<Decorator> for FormatDecorator {
    fn fmt_fields(&self, item: &Decorator, f: &mut PyFormatter) -> FormatResult<()> {
        let Decorator {
            expression,
            range: _,
        } = item;

        write!(
            f,
            [
                token("@"),
                maybe_parenthesize_expression(expression, item, Parenthesize::Optional)
            ]
        )
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
