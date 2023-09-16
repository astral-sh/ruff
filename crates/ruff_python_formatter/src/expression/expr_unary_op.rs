use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprUnaryOp;
use ruff_python_ast::UnaryOp;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::trailing_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
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

        // Split off the comments that follow after the operator and format them as trailing comments.
        // ```python
        // (not # comment
        //      a)
        // ```
        let leading_operand_comments = comments.leading(operand.as_ref());
        let trailing_operator_comments_end =
            leading_operand_comments.partition_point(|p| p.line_position().is_end_of_line());
        let (trailing_operator_comments, leading_operand_comments) =
            leading_operand_comments.split_at(trailing_operator_comments_end);

        if !trailing_operator_comments.is_empty() {
            trailing_comments(trailing_operator_comments).fmt(f)?;
        }

        // Insert a line break if the operand has comments but itself is not parenthesized.
        // ```python
        // if (
        //  not
        //  # comment
        //  a)
        // ```
        if !leading_operand_comments.is_empty()
            && !is_operand_parenthesized(item, f.context().source())
        {
            hard_line_break().fmt(f)?;
        } else if op.is_not() {
            space().fmt(f)?;
        }

        operand.format().fmt(f)
    }
}

impl NeedsParentheses for ExprUnaryOp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        // We preserve the parentheses of the operand. It should not be necessary to break this expression.
        if is_operand_parenthesized(self, context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::Multiline
        }
    }
}

fn is_operand_parenthesized(unary: &ExprUnaryOp, source: &str) -> bool {
    let operator_len = match unary.op {
        UnaryOp::Invert => '~'.text_len(),
        UnaryOp::Not => "not".text_len(),
        UnaryOp::UAdd => '+'.text_len(),
        UnaryOp::USub => '-'.text_len(),
    };

    let trivia_range = TextRange::new(unary.start() + operator_len, unary.operand.start());

    if let Some(token) = SimpleTokenizer::new(source, trivia_range)
        .skip_trivia()
        .next()
    {
        debug_assert_eq!(token.kind(), SimpleTokenKind::LParen);
        true
    } else {
        false
    }
}
