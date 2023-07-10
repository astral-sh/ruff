use crate::comments::{leading_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{group, soft_line_break_or_space, space, text};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use rustpython_parser::ast::ExprIfExp;

#[derive(Default)]
pub struct FormatExprIfExp;

impl FormatNodeRule<ExprIfExp> for FormatExprIfExp {
    fn fmt_fields(&self, item: &ExprIfExp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprIfExp {
            range: _,
            test,
            body,
            orelse,
        } = item;
        let comments = f.context().comments().clone();
        // We place `if test` and `else orelse` on a single line, so the `test` and `orelse` leading
        // comments go on the line before the `if` or `else` instead of directly ahead `test` or
        // `orelse`
        write!(
            f,
            [group(&format_args![
                body.format(),
                soft_line_break_or_space(),
                leading_comments(comments.leading_comments(test.as_ref())),
                text("if"),
                space(),
                test.format(),
                soft_line_break_or_space(),
                leading_comments(comments.leading_comments(orelse.as_ref())),
                text("else"),
                space(),
                orelse.format()
            ])]
        )
    }
}

impl NeedsParentheses for ExprIfExp {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source, comments)
    }
}
