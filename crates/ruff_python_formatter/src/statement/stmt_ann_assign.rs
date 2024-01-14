use ruff_formatter::write;
use ruff_python_ast::StmtAnnAssign;

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::parentheses::Parentheses;
use crate::expression::{has_parentheses, is_splittable_expression};
use crate::prelude::*;
use crate::preview::{
    is_parenthesize_long_type_hints_enabled,
    is_prefer_splitting_right_hand_side_of_assignments_enabled,
};
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
                // The `has_parentheses` check can be removed when stabilizing `is_parenthesize_long_type_hints`.
                // because `is_splittable_expression` covers both.
                && (has_parentheses(annotation, f.context()).is_some()
                    || (is_parenthesize_long_type_hints_enabled(f.context())
                        && is_splittable_expression(annotation, f.context())))
            {
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
                if is_parenthesize_long_type_hints_enabled(f.context()) {
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
                } else {
                    annotation.format().fmt(f)?;
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
            if is_parenthesize_long_type_hints_enabled(f.context()) {
                FormatStatementsLastExpression::left_to_right(annotation, item).fmt(f)?;
            } else {
                annotation.format().fmt(f)?;
            }
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
