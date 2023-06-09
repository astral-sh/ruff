use crate::comments::{dangling_node_comments, Comments};
use crate::context::PyFormatContext;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::trivia::{first_non_trivia_token, TokenKind};
use crate::{AsFormat, FormatNodeRule, FormattedIterExt, PyFormatter, USE_MAGIC_TRAILING_COMMA};
use ruff_formatter::formatter::Formatter;
use ruff_formatter::prelude::{
    block_indent, group, hard_line_break, if_group_breaks, soft_block_indent, soft_line_break,
    soft_line_break_or_space, text,
};
use ruff_formatter::{format_args, write, Buffer, Format, FormatResult};
use ruff_python_ast::prelude::{Expr, Ranged};

use rustpython_parser::ast::ExprTuple;

#[derive(Default)]
pub struct FormatExprTuple;

impl FormatNodeRule<ExprTuple> for FormatExprTuple {
    fn fmt_fields(&self, item: &ExprTuple, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprTuple {
            range: _,
            elts,
            ctx: _,
        } = item;

        // Handle the edge cases of an empty tuple and a tuple with one element
        let last = match &elts[..] {
            [] => {
                return write!(
                    f,
                    [group(&format_args![
                        // A single element tuple always needs parentheses
                        &text("("),
                        block_indent(&dangling_node_comments(item)),
                        &text(")"),
                    ])]
                );
            }
            [single] => {
                return write!(
                    f,
                    [group(&format_args![
                        // A single element tuple always needs parentheses
                        &text("("),
                        soft_block_indent(&format_args![single.format(), &text(",")]),
                        &text(")"),
                    ])]
                );
            }
            [.., last] => last,
        };

        let magic_trailing_comma = USE_MAGIC_TRAILING_COMMA
            && first_non_trivia_token(last.range().end(), f.context().contents())
                .map(|token| token.kind)
                == Some(TokenKind::Comma);

        if magic_trailing_comma {
            // A magic trailing comma forces us to print in expanded mode since we have more than
            // one element
            write!(
                f,
                [
                    // An expanded group always needs parentheses
                    &text("("),
                    hard_line_break(),
                    block_indent(&ExprSequence::new(elts)),
                    hard_line_break(),
                    &text(")"),
                ]
            )?;
        } else {
            write!(
                f,
                [group(&format_args![
                    // If there were previously no parentheses, add them only if the group breaks
                    if_group_breaks(&text("(")),
                    soft_block_indent(&ExprSequence::new(elts)),
                    if_group_breaks(&text(")")),
                ])]
            )?;
        }

        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &ExprTuple, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

#[derive(Debug)]
struct ExprSequence<'a> {
    elts: &'a [Expr],
}

impl<'a> ExprSequence<'a> {
    const fn new(elts: &'a [Expr]) -> Self {
        Self { elts }
    }
}

impl Format<PyFormatContext<'_>> for ExprSequence<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        for (pos, entry) in self.elts.iter().enumerate() {
            // We need a trailing comma on the last entry of an expanded group since we have more
            // than one element
            if pos == self.elts.len() - 1 {
                write!(
                    f,
                    [
                        entry.format(),
                        if_group_breaks(&text(",")),
                        soft_line_break()
                    ]
                )?;
            } else {
                write!(f, [entry.format(), text(","), soft_line_break_or_space()])?;
            }
        }
        Ok(())
    }
}

impl NeedsParentheses for ExprTuple {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
