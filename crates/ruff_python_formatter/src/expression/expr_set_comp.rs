use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprSetComp;

use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSetComp;

impl FormatNodeRule<ExprSetComp> for FormatExprSetComp {
    fn fmt_fields(&self, item: &ExprSetComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSetComp {
            range: _,
            elt,
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
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    joined
                )),
                "}"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for ExprSetComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
