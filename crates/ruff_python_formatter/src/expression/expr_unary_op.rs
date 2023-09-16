use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprUnaryOp;
use ruff_python_ast::UnaryOp;

use crate::comments::{trailing_comments, SourceComment};
use crate::expression::parentheses::{
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprUnaryOp;

impl FormatNodeRule<ExprUnaryOp> for FormatExprUnaryOp {
    fn fmt_fields(&self, item: &ExprUnaryOp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprUnaryOp {
            range: _,
            op,
            operand,
        } = item;

        let operator = match op {
            UnaryOp::Invert => "~",
            UnaryOp::Not => "not",
            UnaryOp::UAdd => "+",
            UnaryOp::USub => "-",
        };

        token(operator).fmt(f)?;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        // Split off the comments that follow after the operator and format them as trailing comments.
        // ```python
        // (not # comment
        //      a)
        // ```
        trailing_comments(dangling).fmt(f)?;

        // Insert a line break if the operand has comments but itself is not parenthesized.
        // ```python
        // if (
        //  not
        //  # comment
        //  a)
        // ```
        if comments.has_leading(operand.as_ref())
            && !is_expression_parenthesized(
                operand.as_ref().into(),
                f.context().comments().ranges(),
                f.context().source(),
            )
        {
            hard_line_break().fmt(f)?;
        } else if op.is_not() {
            space().fmt(f)?;
        }

        operand.format().fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for ExprUnaryOp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        // We preserve the parentheses of the operand. It should not be necessary to break this expression.
        if is_expression_parenthesized(
            self.operand.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::Multiline
        }
    }
}
