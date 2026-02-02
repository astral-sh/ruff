use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprUnaryOp;
use ruff_python_ast::UnaryOp;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_text_size::Ranged;

use crate::comments::trailing_comments;
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, is_expression_parenthesized,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprUnaryOp;

impl FormatNodeRule<ExprUnaryOp> for FormatExprUnaryOp {
    fn fmt_fields(&self, item: &ExprUnaryOp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprUnaryOp {
            range: _,
            node_index: _,
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

        // Insert a line break if the operand has comments but itself is not parenthesized or if the
        // operand is parenthesized but has a leading comment before the parentheses.
        // ```python
        // if (
        //  not
        //  # comment
        //  a):
        //      pass
        //
        // if 1 and (
        //     not
        //     # comment
        //     (
        //         a
        //     )
        // ):
        //     pass
        // ```
        if needs_line_break(item, f.context()) {
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
            return OptionalParentheses::Always;
        }

        if needs_line_break(self, context) {
            return OptionalParentheses::Always;
        }

        if is_expression_parenthesized(
            self.operand.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            return OptionalParentheses::Never;
        }

        if context.comments().has(self.operand.as_ref()) {
            return OptionalParentheses::Always;
        }

        self.operand.needs_parentheses(self.into(), context)
    }
}

/// Returns `true` if the unary operator will have a hard line break between the operator and its
/// operand and thus requires parentheses.
fn needs_line_break(item: &ExprUnaryOp, context: &PyFormatContext) -> bool {
    let comments = context.comments();
    let parenthesized_operand_range = parenthesized_range(
        item.operand.as_ref().into(),
        item.into(),
        comments.ranges(),
        context.source(),
    );
    let leading_operand_comments = comments.leading(item.operand.as_ref());
    let has_leading_comments_before_parens = parenthesized_operand_range.is_some_and(|range| {
        leading_operand_comments
            .iter()
            .any(|comment| comment.start() < range.start())
    });

    !leading_operand_comments.is_empty()
        && !is_expression_parenthesized(
            item.operand.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        )
        || has_leading_comments_before_parens
}
