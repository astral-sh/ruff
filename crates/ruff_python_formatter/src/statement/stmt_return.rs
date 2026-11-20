use ruff_formatter::write;
use ruff_python_ast::{ExprKind, StmtReturn};

use crate::expression::expr_tuple::TupleParentheses;
use crate::prelude::*;
use crate::statement::stmt_assign::FormatStatementsLastExpression;

#[derive(Default)]
pub struct FormatStmtReturn;

impl FormatNodeRule<ruff_python_ast::kind::StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, item: StmtReturn<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let value = item.value();

        token("return").fmt(f)?;

        match value.map(ruff_python_ast::Expr::classify) {
            Some(ExprKind::Tuple(tuple)) if !f.context().comments().has_leading(tuple) => {
                write!(
                    f,
                    [
                        space(),
                        tuple
                            .format()
                            .with_options(TupleParentheses::OptionalParentheses)
                    ]
                )
            }
            Some(value) => {
                write!(
                    f,
                    [
                        space(),
                        FormatStatementsLastExpression::left_to_right(value.view(), item)
                    ]
                )
            }
            None => Ok(()),
        }
    }
}
