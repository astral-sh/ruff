use ruff_formatter::write;
use ruff_python_ast::StmtTypeAlias;

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

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

        write!(f, [token("type"), space(), name.as_ref().format()])?;

        if let Some(type_params) = type_params {
            write!(f, [type_params.format()])?;
        }

        write!(
            f,
            [
                space(),
                token("="),
                space(),
                maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
            ]
        )
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
