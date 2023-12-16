use ruff_formatter::write;
use ruff_python_ast::StmtAnnAssign;

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::has_parentheses;
use crate::prelude::*;
use crate::preview::is_prefer_splitting_right_hand_side_of_assignments_enabled;
use crate::statement::stmt_assign::{
    AnyAssignmentOperator, AnyBeforeOperator, FormatStatementsLastExpression,
};
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

        write!(f, [target.format(), token(":"), space()])?;

        if let Some(value) = value {
            if is_prefer_splitting_right_hand_side_of_assignments_enabled(f.context())
                && has_parentheses(annotation, f.context()).is_some()
            {
                FormatStatementsLastExpression::RightToLeft {
                    before_operator: AnyBeforeOperator::Expression(annotation),
                    operator: AnyAssignmentOperator::Assign,
                    value,
                    statement: item.into(),
                }
                .fmt(f)?;
            } else {
                write!(
                    f,
                    [
                        annotation.format(),
                        space(),
                        token("="),
                        space(),
                        FormatStatementsLastExpression::left_to_right(value, item)
                    ]
                )?;
            }
        } else {
            annotation.format().fmt(f)?;
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
