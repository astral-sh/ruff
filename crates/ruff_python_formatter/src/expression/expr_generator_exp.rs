use crate::context::PyFormatContext;
use crate::expression::parentheses::parenthesized;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprGeneratorExp;

#[derive(Default)]
pub struct FormatExprGeneratorExp;

impl FormatNodeRule<ExprGeneratorExp> for FormatExprGeneratorExp {
    fn fmt_fields(&self, item: &ExprGeneratorExp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprGeneratorExp {
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
                "(",
                &format_args!(
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    group(&joined)
                ),
                ")"
            )]
        )
    }
}

impl NeedsParentheses for ExprGeneratorExp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
