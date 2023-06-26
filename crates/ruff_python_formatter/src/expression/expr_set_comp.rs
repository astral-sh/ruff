use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprSetComp;

#[derive(Default)]
pub struct FormatExprSetComp;

impl FormatNodeRule<ExprSetComp> for FormatExprSetComp {
    fn fmt_fields(&self, _item: &ExprSetComp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "{NOT_IMPLEMENTED_set_value for value in NOT_IMPLEMENTED_set}"
            )]
        )
    }
}

impl NeedsParentheses for ExprSetComp {
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
