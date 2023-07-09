use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
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
        if let Some(msg) = msg {
            write!(
                f,
                [
                    text("assert"),
                    space(),
                    test.format().with_options(Parenthesize::IfBreaks),
                    text(","),
                    space(),
                    msg.format().with_options(Parenthesize::IfBreaks)
                ]
            )
        } else {
            write!(
                f,
                [
                    text("assert"),
                    space(),
                    test.format().with_options(Parenthesize::IfBreaks)
                ]
            )
        }
    }
}
