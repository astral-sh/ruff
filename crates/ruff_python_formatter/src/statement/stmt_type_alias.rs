use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::StmtTypeAlias;

#[derive(Default)]
pub struct FormatStmtTypeAlias;

impl FormatNodeRule<StmtTypeAlias> for FormatStmtTypeAlias {
    fn fmt_fields(&self, item: &StmtTypeAlias, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtTypeAlias {
            name,
            type_params,
            value,
            range: _,
        } = item;

        write!(f, [text("type"), space(), name.as_ref().format()])?;

        if let Some(type_params) = type_params {
            write!(f, [type_params.format()])?;
        }

        write!(
            f,
            [
                space(),
                text("="),
                space(),
                maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
            ]
        )
    }
}
