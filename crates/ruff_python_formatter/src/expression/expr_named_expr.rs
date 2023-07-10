use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprNamedExpr;

#[derive(Default)]
pub struct FormatExprNamedExpr;

impl FormatNodeRule<ExprNamedExpr> for FormatExprNamedExpr {
    fn fmt_fields(&self, item: &ExprNamedExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprNamedExpr {
            target,
            value,
            range: _,
        } = item;
        write!(
            f,
            [
                target.format(),
                space(),
                text(":="),
                space(),
                value.format(),
            ]
        )
    }
}

impl NeedsParentheses for ExprNamedExpr {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            // Unlike tuples, named expression parentheses are not part of the range even when
            // mandatory. See [PEP 572](https://peps.python.org/pep-0572/) for details.
            Parentheses::Optional => Parentheses::Always,
            parentheses => parentheses,
        }
    }
}
