use crate::context::PyFormatContext;
use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprListComp;

#[derive(Default)]
pub struct FormatExprListComp;

impl FormatNodeRule<ExprListComp> for FormatExprListComp {
    fn fmt_fields(&self, item: &ExprListComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprListComp {
            range: _,
            elt,
            generators,
        } = item;

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        write!(
            f,
            [parenthesized(
                "[",
                &group(&format_args![
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    group(&joined)
                ]),
                "]"
            )]
        )
    }
}

impl NeedsParentheses for ExprListComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
