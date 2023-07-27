use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::StmtAssert;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssert {
            range: _,
            test,
            msg,
        } = item;

        write!(
            f,
            [
                text("assert"),
                space(),
                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks)
            ]
        )?;

        if let Some(msg) = msg {
            write!(
                f,
                [
                    text(","),
                    space(),
                    maybe_parenthesize_expression(msg, item, Parenthesize::IfBreaks),
                ]
            )?;
        }

        Ok(())
    }
}
