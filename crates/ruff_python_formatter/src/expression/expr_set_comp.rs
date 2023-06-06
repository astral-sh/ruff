use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprSetComp;

#[derive(Default)]
pub struct FormatExprSetComp;

impl FormatNodeRule<ExprSetComp> for FormatExprSetComp {
    fn fmt_fields(&self, item: &ExprSetComp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}

impl NeedsParentheses for ExprSetComp {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
