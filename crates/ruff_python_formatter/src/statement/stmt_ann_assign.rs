use ruff_formatter::write;
use ruff_python_ast::StmtAnnAssign;

use crate::comments::{SourceComment, SuppressionKind};
use crate::prelude::*;
use crate::statement::stmt_assign::FormatStatementsLastExpression;
use crate::statement::trailing_semicolon;

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
            [target.format(), token(":"), space(), annotation.format(),]
        )?;

        if let Some(value) = value {
            write!(
                f,
                [
                    space(),
                    token("="),
                    space(),
                    FormatStatementsLastExpression::new(value, item)
                ]
            )?;
        }

        if f.options().source_type().is_ipynb()
            && f.context().node_level().is_last_top_level_statement()
            && target.is_name_expr()
            && trailing_semicolon(item.into(), f.context().source()).is_some()
        {
            token(";").fmt(f)?;
        }

        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
