use ruff_formatter::write;
use ruff_python_ast::StmtAnnAssign;

use crate::comments::SourceComment;
use crate::expression::is_splittable_expression;
use crate::expression::parentheses::Parentheses;
use crate::statement::stmt_assign::{
    AnyAssignmentOperator, AnyBeforeOperator, FormatStatementsLastExpression,
};
use crate::statement::trailing_semicolon;
use crate::{has_skip_comment, prelude::*};

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
            if is_splittable_expression(annotation, f.context()) {
                FormatStatementsLastExpression::RightToLeft {
                    before_operator: AnyBeforeOperator::Expression(annotation),
                    operator: AnyAssignmentOperator::Assign,
                    value,
                    statement: item.into(),
                }
                .fmt(f)?;
            } else {
                // Remove unnecessary parentheses around the annotation if the parenthesize long type hints preview style is enabled.
                // Ensure we keep the parentheses if the annotation has any comments.
                if f.context().comments().has_leading(annotation.as_ref())
                    || f.context().comments().has_trailing(annotation.as_ref())
                {
                    annotation
                        .format()
                        .with_options(Parentheses::Always)
                        .fmt(f)?;
                } else {
                    annotation
                        .format()
                        .with_options(Parentheses::Never)
                        .fmt(f)?;
                }

                write!(
                    f,
                    [
                        space(),
                        token("="),
                        space(),
                        FormatStatementsLastExpression::left_to_right(value, item)
                    ]
                )?;
            }
        } else {
            // Parenthesize the value and inline the comment if it is a "simple" type annotation, similar
            // to what we do with the value.
            // ```python
            // class Test:
            //     safe_age: (
            //         Decimal  #  the user's age, used to determine if it's safe for them to use ruff
            //     )
            // ```
            FormatStatementsLastExpression::left_to_right(annotation, item).fmt(f)?;
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
