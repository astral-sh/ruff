use ruff_formatter::write;
use ruff_python_ast::Decorator;

use crate::comments::has_skip_comment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::verbatim::suppressed_node;

#[derive(Default)]
pub struct FormatDecorator;

impl FormatNodeRule<Decorator> for FormatDecorator {
    fn fmt_fields(&self, item: &Decorator, f: &mut PyFormatter) -> FormatResult<()> {
        if has_skip_comment(f.context().comments().trailing(item), f.context().source()) {
            suppressed_node(item).fmt(f)
        } else {
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
    }
}
