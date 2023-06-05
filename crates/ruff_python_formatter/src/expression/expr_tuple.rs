use crate::context::{NodeLevel, PyFormatContext};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::trivia::find_first_non_trivia_character_after;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::formatter::Formatter;
use ruff_formatter::prelude::{
    block_indent, group, hard_line_break, if_group_breaks, soft_block_indent, soft_line_break,
    soft_line_break_or_space, text,
};
use ruff_formatter::{format_args, write, Buffer, Format, FormatResult};
use ruff_python_ast::prelude::{Expr, Ranged};
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::ExprTuple;

/// TODO(konstin): hook this up to the setting
const USE_MAGIC_TRAILING_COMMA: bool = true;

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
                text("()").fmt(f)?;
                return Ok(());
            }
            [single] => {
                write!(
                    f,
                    [group(&format_args![
                        &text("("),
                        soft_block_indent(&group(&format_args![single.format(), &text(",")])),
                        &text(")"),
                    ])]
                )?;
                return Ok(());
            }
            [.., last] => last,
        };

        let saved_level = f.context().node_level();
        // Tell the children they need parentheses
        f.context_mut().set_node_level(NodeLevel::Expression);

        let magic_trailing_comma = USE_MAGIC_TRAILING_COMMA
            && matches!(
                find_first_non_trivia_character_after(last.range().end(), f.context().contents()),
                Some((_, ','))
            );

        if magic_trailing_comma {
            // A magic trailing comma forces us to print in expanded mode since we have more than
            // one element
            write!(
                f,
                [group(&format_args![
                    &text("("),
                    hard_line_break(),
                    block_indent(&ExprSequence::new(elts)),
                    hard_line_break(),
                    &text(")"),
                ])]
            )?;
            return Ok(());
        } else if is_parenthesized(*range, elts, f) || saved_level != NodeLevel::TopLevel {
            // If the tuple has parentheses, keep them. Also only top level tuples are allowed to
            // elide them
            write!(
                f,
                [group(&format_args![
                    &text("("),
                    soft_block_indent(&ExprSequence::new(elts)),
                    &text(")"),
                ])]
            )?;
        } else {
            write!(
                f,
                [group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&ExprSequence::new(elts)),
                    if_group_breaks(&text(")")),
                ])]
            )?;
        }

        f.context_mut().set_node_level(saved_level);
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

fn is_parenthesized(
    range: TextRange,
    elts: &[Expr],
    f: &mut Formatter<PyFormatContext<'_>>,
) -> bool {
    let parentheses = "(";
    let first_char = &f.context().contents()[TextRange::at(range.start(), parentheses.text_len())];
    if first_char != parentheses {
        return false;
    }

    // Consider `a = (1, 2), 3`: The first char of the current expr starts is a parentheses, but
    // it's not its own but that of its first tuple child. We know that it belongs to the child
    // because if it wouldn't, the child would start (at least) a char later
    let Some(first_child) = elts.first() else {
        return false;
    };
    first_child.range().start() != range.start()
}

impl NeedsParentheses for ExprTuple {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
