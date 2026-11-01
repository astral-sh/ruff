use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{ExprUnaryOp, SyntaxNode, UnaryOp};
use ruff_text_size::Ranged;

use crate::comments::trailing_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprUnaryOp;

impl FormatNodeRule<ruff_python_ast::kind::ExprUnaryOp> for FormatExprUnaryOp {
    fn fmt_fields(&self, item: ExprUnaryOp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let op = item.op();
        let operand = item.operand();

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
            .is_some_and(|bin_op| bin_op.op().is_pow())
        {
            operand.format().with_options(Parentheses::Always).fmt(f)
        } else {
            operand.format().fmt(f)
        }
    }
}

impl NeedsParentheses for ExprUnaryOp<'_> {
    fn needs_parentheses(
        &self,
        parent: SyntaxNode,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            return OptionalParentheses::Always;
        }

        if needs_line_break(*self, context) {
            return OptionalParentheses::Always;
        }

        if context.is_expression_parenthesized(self.operand()) {
            return OptionalParentheses::Never;
        }

        if context.comments().has(self.operand()) {
            return OptionalParentheses::Always;
        }

        self.operand().needs_parentheses(self.into(), context)
    }
}

/// Returns `true` if the unary operator will have a hard line break between the operator and its
/// operand and thus requires parentheses.
fn needs_line_break(item: ExprUnaryOp<'_>, context: &PyFormatContext) -> bool {
    let comments = context.comments();
    let parenthesized_operand_range =
        parenthesized_range(item.operand(), item.into(), context.tokens());
    let leading_operand_comments = comments.leading(item.operand());
    let has_leading_comments_before_parens = parenthesized_operand_range.is_some_and(|range| {
        leading_operand_comments
            .iter()
            .any(|comment| comment.start() < range.start())
    });

    !leading_operand_comments.is_empty() && !context.is_expression_parenthesized(item.operand())
        || has_leading_comments_before_parens
}
