use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use ruff_python_ast::StmtAnnAssign;

#[derive(Default)]
pub struct FormatStmtAnnAssign;

impl FormatNodeRule<StmtAnnAssign> for FormatStmtAnnAssign {
    fn fmt_fields(&self, item: &StmtAnnAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAnnAssign {
            range: _,
            target,
            annotation,
            value,
            simple: _,
        } = item;

        write!(
            f,
            [
                target.format(),
                text(":"),
                space(),
                maybe_parenthesize_expression(annotation, item, Parenthesize::IfBreaks)
            ]
        )?;

        if let Some(value) = value {
            write!(
                f,
                [
                    space(),
                    text("="),
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
                ]
            )?;
        }

        Ok(())
    }
}
