use crate::comments::dangling_comments;
use crate::context::PyFormatContext;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{soft_line_break, space, text};
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

        write!(f, [target.format()])?;

        let comments = f.context().comments().clone();
        let trailing_target_comments = comments.trailing_comments(target.as_ref());
        let leading_value_comments = comments.leading_comments(value.as_ref());
        let dangling_item_comments = comments.dangling_comments(item);

        if trailing_target_comments.is_empty() {
            write!(f, [space()])?;
        } else {
            write!(f, [soft_line_break()])?;
        }

        write!(f, [text(":="), dangling_comments(dangling_item_comments)])?;

        if leading_value_comments.is_empty() {
            write!(f, [space()])?;
        } else {
            write!(f, [soft_line_break()])?;
        }
        write!(f, [value.format()])
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExprNamedExpr,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprNamedExpr {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        context: &PyFormatContext,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, context) {
            // Unlike tuples, named expression parentheses are not part of the range even when
            // mandatory. See [PEP 572](https://peps.python.org/pep-0572/) for details.
            Parentheses::Optional => Parentheses::Always,
            parentheses => parentheses,
        }
    }
}
