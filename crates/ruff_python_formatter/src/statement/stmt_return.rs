use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::StmtReturn;

#[derive(Default)]
pub struct FormatStmtReturn;

impl FormatNodeRule<StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, item: &StmtReturn, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtReturn { range: _, value } = item;
        if let Some(value) = value {
            write!(
                f,
                [
                    text("return"),
                    space(),
                    value.format().with_options(Parenthesize::IfBreaks)
                ]
            )
        } else {
            text("return").fmt(f)
        }
    }
}
