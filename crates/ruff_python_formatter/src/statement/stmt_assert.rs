use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{format_args, group, space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAssert;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssert {
            range: _,
            test,
            msg,
        } = item;

        write!(f, [text("assert"), space()])?;

        if let Some(msg) = msg {
            write!(
                f,
                [group(&format_args![
                    maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                    text(","),
                    space(),
                    maybe_parenthesize_expression(msg, item, Parenthesize::IfBreaks),
                ])]
            )?;
        } else {
            write!(
                f,
                [maybe_parenthesize_expression(
                    test,
                    item,
                    Parenthesize::IfBreaks
                )]
            )?;
        }

        Ok(())
    }
}
