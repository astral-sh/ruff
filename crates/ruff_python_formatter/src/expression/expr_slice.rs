use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprSlice;

#[derive(Default)]
pub struct FormatExprSlice;

impl FormatNodeRule<ExprSlice> for FormatExprSlice {
    fn fmt_fields(&self, _item: &ExprSlice, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "NOT_IMPLEMENTED_start:NOT_IMPLEMENTED_end"
            )]
        )
    }
}

impl NeedsParentheses for ExprSlice {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source)
    }
}
