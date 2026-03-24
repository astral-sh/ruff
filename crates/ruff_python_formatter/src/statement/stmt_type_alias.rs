use ruff_formatter::write;
use ruff_python_ast::StmtTypeAlias;

use crate::expression::is_invalid_type_expression;
use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use crate::statement::stmt_assign::{
    AnyAssignmentOperator, AnyBeforeOperator, FormatStatementsLastExpression,
};

#[derive(Default)]
pub struct FormatStmtTypeAlias;

impl FormatNodeRule<StmtTypeAlias> for FormatStmtTypeAlias {
    fn fmt_fields(&self, item: &StmtTypeAlias, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtTypeAlias {
            name,
            type_params,
            value,
            range: _,
            node_index: _,
        } = item;

        write!(f, [token("type"), space(), name.as_ref().format()])?;

        if is_invalid_type_expression(value) {
            if let Some(type_params) = type_params {
                type_params.format().fmt(f)?;
            }

            return write!(
                f,
                [
                    space(),
                    token("="),
                    space(),
                    value.format().with_options(Parentheses::Preserve)
                ]
            );
        }

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
}
