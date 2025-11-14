use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprUnaryOp;
use ruff_python_ast::UnaryOp;
use ruff_python_trivia::SimpleTokenKind;
use ruff_python_trivia::SimpleTokenizer;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::comments::leading_comments;
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

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let up_to = operand_start(item, f.context().source());

        let pivot = dangling.partition_point(|comment| comment.end() < up_to);
        let leading = &dangling[..pivot];

        // Split off the comments that follow after the operator and format them as trailing comments.
        // ```python
        // (not # comment
        //      a)
        // ```
        leading_comments(leading).fmt(f)?;
        trailing_comments(&dangling[pivot..]).fmt(f)?;

        let operator = match op {
            UnaryOp::Invert => "~",
            UnaryOp::Not => "not",
            UnaryOp::UAdd => "+",
            UnaryOp::USub => "-",
        };

        token(operator).fmt(f)?;

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
        } else if context
            .comments()
            .dangling(self)
            .iter()
            .any(|comment| comment.end() < operand_start(self, context.source()))
        {
            OptionalParentheses::Multiline
        } else if context.comments().has(self.operand.as_ref()) {
            OptionalParentheses::Always
        } else {
            self.operand.needs_parentheses(self.into(), context)
        }
    }
}

/// Returns the start of `unary_op`'s operand, or its leading parenthesis, if it has one.
pub(crate) fn operand_start(unary_op: &ExprUnaryOp, source: &str) -> TextSize {
    let mut tokenizer = SimpleTokenizer::new(
        source,
        TextRange::new(unary_op.start(), unary_op.operand.start()),
    )
    .skip_trivia();
    let op_token = tokenizer.next();
    debug_assert!(op_token.is_some_and(|token| matches!(
        token.kind,
        SimpleTokenKind::Tilde
            | SimpleTokenKind::Not
            | SimpleTokenKind::Plus
            | SimpleTokenKind::Minus
    )));
    tokenizer
        .find(|token| token.kind == SimpleTokenKind::LParen)
        .map_or(unary_op.operand.start(), |lparen| lparen.start())
}
