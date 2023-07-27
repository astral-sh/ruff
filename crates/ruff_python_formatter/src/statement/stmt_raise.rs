use crate::expression::parentheses::Parenthesize;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};

use crate::expression::maybe_parenthesize_expression;
use ruff_python_ast::StmtRaise;

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
                [
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::Optional)
                ]
            )?;
        }

        if let Some(value) = cause {
            write!(
                f,
                [
                    space(),
                    text("from"),
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::Optional)
                ]
            )?;
        }
        Ok(())
    }
}
