use ruff_formatter::prelude::{space, token};
use ruff_formatter::write;
use ruff_python_ast::StmtAssert;
use ruff_python_ast::flat;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(
        &self,
        item: flat::View<'_, StmtAssert>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let test = item.test();
        let msg = item.msg();

        write!(
            f,
            [
                token("assert"),
                space(),
                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks)
            ]
        )?;

        if let Some(msg) = msg {
            write!(
                f,
                [
                    token(","),
                    space(),
                    maybe_parenthesize_expression(msg, item, Parenthesize::IfBreaksParenthesized),
                ]
            )?;
        }

        Ok(())
    }
}
