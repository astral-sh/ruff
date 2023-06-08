use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprLambda;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, _item: &ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented_custom_text("lambda x: True")])
    }
}

impl NeedsParentheses for ExprLambda {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source, comments)
    }
}
