use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

use crate::comments::Comments;
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprSubscript;

#[derive(Default)]
pub struct FormatExprSubscript;

impl FormatNodeRule<ExprSubscript> for FormatExprSubscript {
    fn fmt_fields(&self, _item: &ExprSubscript, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "NOT_IMPLEMENTED_value[NOT_IMPLEMENTED_key]"
            )]
        )
    }
}

impl NeedsParentheses for ExprSubscript {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source, comments)
    }
}
