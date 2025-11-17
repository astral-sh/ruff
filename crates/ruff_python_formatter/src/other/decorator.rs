use ruff_formatter::write;
use ruff_python_ast::Decorator;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatDecorator;

impl FormatNodeRule<Decorator> for FormatDecorator {
    fn fmt_fields(&self, item: &Decorator, f: &mut PyFormatter) -> FormatResult<()> {
        let Decorator {
            expression,
            range: _,
            node_index: _,
        } = item;

        write!(
            f,
            [
                token("@"),
                maybe_parenthesize_expression(expression, item, Parenthesize::Optional)
            ]
        )
    }
    fn is_suppressed(&self, node: &Decorator, context: &PyFormatContext) -> bool {
        context.is_suppressed(node.into())
    }
}
