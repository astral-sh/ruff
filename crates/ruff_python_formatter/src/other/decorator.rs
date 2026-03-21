use ruff_formatter::write;
use ruff_python_ast::Decorator;
use ruff_text_size::Ranged;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::verbatim::verbatim_text;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatDecorator;

impl FormatNodeRule<Decorator> for FormatDecorator {
    fn fmt_fields(&self, item: &Decorator, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments();
        let trailing = comments.trailing(item);

        if has_skip_comment(trailing, f.context().source()) {
            comments.mark_verbatim_node_comments_formatted(item.into());

            verbatim_text(item.range()).fmt(f)
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
