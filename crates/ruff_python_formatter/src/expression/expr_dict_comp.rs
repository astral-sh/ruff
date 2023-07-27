use crate::context::PyFormatContext;
use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::AsFormat;
use crate::{FormatNodeRule, FormattedIterExt, PyFormatter};
use ruff_formatter::prelude::{
    format_args, format_with, group, soft_line_break_or_space, space, text,
};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprDictComp;

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

        write!(
            f,
            [parenthesized(
                "{",
                &group(&format_args!(
                    group(&key.format()),
                    text(":"),
                    space(),
                    value.format(),
                    soft_line_break_or_space(),
                    group(&joined)
                )),
                "}"
            )]
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
