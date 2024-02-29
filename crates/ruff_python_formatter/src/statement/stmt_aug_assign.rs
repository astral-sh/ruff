use ruff_formatter::write;
use ruff_python_ast::StmtAugAssign;

use crate::comments::SourceComment;
use crate::expression::parentheses::is_expression_parenthesized;
use crate::statement::stmt_assign::{
    has_target_own_parentheses, AnyAssignmentOperator, AnyBeforeOperator,
    FormatStatementsLastExpression,
};
use crate::statement::trailing_semicolon;
use crate::{has_skip_comment, prelude::*};
use crate::{AsFormat, FormatNodeRule};

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

        if has_target_own_parentheses(target, f.context())
            && !is_expression_parenthesized(
                target.into(),
                f.context().comments().ranges(),
                f.context().source(),
            )
        {
            FormatStatementsLastExpression::RightToLeft {
                before_operator: AnyBeforeOperator::Expression(target),
                operator: AnyAssignmentOperator::AugAssign(*op),
                value,
                statement: item.into(),
            }
            .fmt(f)?;
        } else {
            write!(
                f,
                [
                    target.format(),
                    space(),
                    op.format(),
                    token("="),
                    space(),
                    FormatStatementsLastExpression::left_to_right(value, item)
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
        has_skip_comment(trailing_comments, context.source())
    }
}
