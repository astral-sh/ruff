use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprListComp;

#[derive(Default)]
pub struct FormatExprListComp;

impl FormatNodeRule<ExprListComp> for FormatExprListComp {
    fn fmt_fields(&self, _item: &ExprListComp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "[NOT_YET_IMPLEMENTED_generator_key for NOT_YET_IMPLEMENTED_generator_key in []]"
            )]
        )
    }
}

impl NeedsParentheses for ExprListComp {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
