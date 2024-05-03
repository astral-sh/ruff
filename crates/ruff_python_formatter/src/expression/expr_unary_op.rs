use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprUnaryOp;
use ruff_python_ast::UnaryOp;

use crate::comments::trailing_comments;
use crate::expression::parentheses::{
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
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

        if operand
            .as_bin_op_expr()
            .is_some_and(|bin_op| bin_op.op.is_pow())
        {
            operand.format().with_options(Parentheses::Always).fmt(f)
        } else {
            operand.format().fmt(f)
        }
    }
}

impl NeedsParentheses for ExprUnaryOp {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else if is_expression_parenthesized(
            self.operand.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            OptionalParentheses::Never
        } else if context.comments().has(self.operand.as_ref()) {
            OptionalParentheses::Always
        } else {
            self.operand.needs_parentheses(self.into(), context)
        }
    }
}
