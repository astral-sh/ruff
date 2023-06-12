use crate::comments::{dangling_node_comments, Comments};
use crate::context::PyFormatContext;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::trivia::Token;
use crate::trivia::{first_non_trivia_token, TokenKind};
use crate::{AsFormat, FormatNodeRule, FormattedIterExt, PyFormatter, USE_MAGIC_TRAILING_COMMA};
use ruff_formatter::formatter::Formatter;
use ruff_formatter::prelude::{
    block_indent, group, if_group_breaks, soft_block_indent, soft_line_break_or_space, text,
};
use ruff_formatter::{format_args, write, Buffer, Format, FormatResult};
use ruff_python_ast::prelude::{Expr, Ranged};
use ruff_text_size::TextRange;
use rustpython_parser::ast::ExprTuple;

#[derive(Default)]
pub struct FormatExprTuple;

impl FormatNodeRule<ExprTuple> for FormatExprTuple {
    fn fmt_fields(&self, item: &ExprTuple, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprTuple {
            range,
            elts,
            ctx: _,
        } = item;

        // Handle the edge cases of an empty tuple and a tuple with one element
        let last = match &elts[..] {
            [] => {
                return write!(
                    f,
                    [
                        // A single element tuple always needs parentheses
                        &text("("),
                        block_indent(&dangling_node_comments(item)),
                        &text(")"),
                    ]
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
            && matches!(
                first_non_trivia_token(last.range().end(), f.context().contents()),
                Some(Token {
                    kind: TokenKind::Comma,
                    ..
                })
            );

        if magic_trailing_comma {
            // A magic trailing comma forces us to print in expanded mode since we have more than
            // one element
            write!(
                f,
                [
                    // An expanded group always needs parentheses
                    &text("("),
                    block_indent(&ExprSequence::new(elts)),
                    &text(")"),
                ]
            )?;
        } else if is_parenthesized(*range, elts, f) {
            // If the tuple has parentheses, keep them. Note that unlike other expr parentheses,
            // those are actually part of the range
            write!(
                f,
                [group(&format_args![
                    // If it was previously parenthesized, add them again
                    &text("("),
                    soft_block_indent(&ExprSequence::new(elts)),
                    &text(")"),
                ])]
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
        f.join_with(&format_args!(text(","), soft_line_break_or_space()))
            .entries(self.elts.iter().formatted())
            .finish()?;
        // We need a trailing comma on the last entry of an expanded group since we have more
        // than one element
        write!(f, [if_group_breaks(&text(","))])
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

/// Check if a tuple has already had parentheses in the input
fn is_parenthesized(
    tuple_range: TextRange,
    elts: &[Expr],
    f: &mut Formatter<PyFormatContext<'_>>,
) -> bool {
    let parentheses = '(';
    let first_char = &f.context().contents()[usize::from(tuple_range.start())..]
        .chars()
        .next();
    let Some(first_char) = first_char else {
        return false;
    };
    if *first_char != parentheses {
        return false;
    }

    // Consider `a = (1, 2), 3`: The first char of the current expr starts is a parentheses, but
    // it's not its own but that of its first tuple child. We know that it belongs to the child
    // because if it wouldn't, the child would start (at least) a char later
    let Some(first_child) = elts.first() else {
        return false;
    };
    first_child.range().start() != tuple_range.start()
}
