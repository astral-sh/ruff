use crate::comments::{trailing_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::trivia::{SimpleTokenizer, TokenKind};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{hard_line_break, space, text};
use ruff_formatter::{Format, FormatContext, FormatResult};
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::UnaryOp;
use rustpython_parser::ast::{ExprUnaryOp, Ranged};

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

        text(operator).fmt(f)?;

        let comments = f.context().comments().clone();

        // Split off the comments that follow after the operator and format them as trailing comments.
        // ```python
        // (not # comment
        //      a)
        // ```
        let leading_operand_comments = comments.leading_comments(operand.as_ref());
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
            && !is_operand_parenthesized(item, f.context().source_code().as_str())
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
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => {
                // We preserve the parentheses of the operand. It should not be necessary to break this expression.
                if is_operand_parenthesized(self, source) {
                    Parentheses::Never
                } else {
                    Parentheses::Optional
                }
            }
            parentheses => parentheses,
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

    let trivia_range = TextRange::new(unary.range.start() + operator_len, unary.operand.start());

    if let Some(token) = SimpleTokenizer::new(source, trivia_range)
        .skip_trivia()
        .next()
    {
        debug_assert_eq!(token.kind(), TokenKind::LParen);
        true
    } else {
        false
    }
}
