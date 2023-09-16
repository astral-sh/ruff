use ruff_formatter::prelude::{
    format_args, format_with, group, soft_line_break_or_space, space, token,
};
use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprDictComp;

use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprDictComp;

impl FormatNodeRule<ExprDictComp> for FormatExprDictComp {
    fn fmt_fields(&self, item: &ExprDictComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprDictComp {
            range: _,
            key,
            value,
            generators,
        } = item;

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(
            f,
            [parenthesized(
                "{",
                &group(&format_args!(
                    group(&key.format()),
                    token(":"),
                    space(),
                    value.format(),
                    soft_line_break_or_space(),
                    joined
                )),
                "}"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for ExprDictComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
