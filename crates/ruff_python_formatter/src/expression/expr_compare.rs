use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprCompare;

#[derive(Default)]
pub struct FormatExprCompare;

impl FormatNodeRule<ExprCompare> for FormatExprCompare {
    fn fmt_fields(&self, _item: &ExprCompare, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "NOT_IMPLEMENTED_left < NOT_IMPLEMENTED_right"
            )]
        )
    }
}

impl NeedsParentheses for ExprCompare {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source)
    }
}
