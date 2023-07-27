use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use ruff_python_ast::Decorator;

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
                text("@"),
                maybe_parenthesize_expression(expression, item, Parenthesize::Optional)
            ]
        )
    }
}
