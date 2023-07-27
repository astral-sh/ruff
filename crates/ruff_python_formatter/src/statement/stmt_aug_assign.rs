use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::StmtAugAssign;

#[derive(Default)]
pub struct FormatStmtAugAssign;

impl FormatNodeRule<StmtAugAssign> for FormatStmtAugAssign {
    fn fmt_fields(&self, item: &StmtAugAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAugAssign {
            target,
            op,
            value,
            range: _,
        } = item;
        write!(
            f,
            [
                target.format(),
                space(),
                op.format(),
                text("="),
                space(),
                maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
            ]
        )
    }
}
