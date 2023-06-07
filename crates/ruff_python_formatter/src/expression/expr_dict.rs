use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprDict;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, item: &ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}

impl NeedsParentheses for ExprDict {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
