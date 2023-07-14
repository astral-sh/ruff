use rustpython_parser::ast::ExprAwait;

use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parenthesize};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatExprAwait;

impl FormatNodeRule<ExprAwait> for FormatExprAwait {
    fn fmt_fields(&self, item: &ExprAwait, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAwait { range: _, value } = item;

        let format_value = format_with(|f: &mut PyFormatter| {
            if f.context().node_level().is_parenthesized() {
                value.format().fmt(f)
            } else {
                maybe_parenthesize_expression(value, item, Parenthesize::Optional).fmt(f)
            }
        });

        write!(f, [text("await"), space(), format_value])
    }
}

impl NeedsParentheses for ExprAwait {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
