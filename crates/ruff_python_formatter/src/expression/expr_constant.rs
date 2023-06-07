use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprConstant;

#[derive(Default)]
pub struct FormatExprConstant;

impl FormatNodeRule<ExprConstant> for FormatExprConstant {
    fn fmt_fields(&self, item: &ExprConstant, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}

impl NeedsParentheses for ExprConstant {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source)
    }
}
