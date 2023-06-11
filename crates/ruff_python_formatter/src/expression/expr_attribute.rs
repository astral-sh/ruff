use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::{not_yet_implemented_custom_text, FormatNodeRule};
use ruff_formatter::write;
use rustpython_parser::ast::{Constant, Expr, ExprAttribute, ExprConstant};

#[derive(Default)]
pub struct FormatExprAttribute;

impl FormatNodeRule<ExprAttribute> for FormatExprAttribute {
    fn fmt_fields(&self, item: &ExprAttribute, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAttribute {
            value,
            range: _,
            attr: _,
            ctx: _,
        } = item;

        let requires_space = matches!(
            value.as_ref(),
            Expr::Constant(ExprConstant {
                value: Constant::Int(_) | Constant::Float(_),
                ..
            })
        );

        write!(
            f,
            [
                item.value.format(),
                requires_space.then_some(space()),
                text("."),
                not_yet_implemented_custom_text("NOT_IMPLEMENTED_attr")
            ]
        )
    }
}

impl NeedsParentheses for ExprAttribute {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source, comments)
    }
}
