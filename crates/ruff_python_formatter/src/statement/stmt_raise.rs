use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};

use rustpython_parser::ast::StmtRaise;

#[derive(Default)]
pub struct FormatStmtRaise;

impl FormatNodeRule<StmtRaise> for FormatStmtRaise {
    fn fmt_fields(&self, item: &StmtRaise, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtRaise {
            range: _,
            exc,
            cause,
        } = item;

        text("raise").fmt(f)?;

        if let Some(value) = exc {
            write!(
                f,
                [space(), value.format().with_options(Parenthesize::IfBreaks)]
            )?;
        }

        if let Some(value) = cause {
            write!(
                f,
                [
                    space(),
                    text("from"),
                    space(),
                    value.format().with_options(Parenthesize::IfBreaks)
                ]
            )?;
        }
        Ok(())
    }
}
