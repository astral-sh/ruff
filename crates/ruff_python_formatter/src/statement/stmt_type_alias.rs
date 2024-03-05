use ruff_formatter::write;
use ruff_python_ast::StmtTypeAlias;

use crate::comments::SourceComment;
use crate::statement::stmt_assign::{
    AnyAssignmentOperator, AnyBeforeOperator, FormatStatementsLastExpression,
};
use crate::{has_skip_comment, prelude::*};

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
            return FormatStatementsLastExpression::RightToLeft {
                before_operator: AnyBeforeOperator::TypeParams(type_params),
                operator: AnyAssignmentOperator::Assign,
                value,
                statement: item.into(),
            }
            .fmt(f);
        }

        write!(
            f,
            [
                space(),
                token("="),
                space(),
                FormatStatementsLastExpression::left_to_right(value, item)
            ]
        )
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
