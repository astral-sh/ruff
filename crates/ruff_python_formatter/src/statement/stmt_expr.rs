use crate::expression::parentheses::{is_expression_parenthesized, Parenthesize};
use crate::expression::string::StringLayout;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_python_ast::expression::ExpressionRef;
use rustpython_parser::ast::StmtExpr;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtExpr { value, .. } = item;

        if let Some(constant) = value.as_constant_expr() {
            if constant.value.is_str()
                && !is_expression_parenthesized(
                    ExpressionRef::from(value.as_ref()),
                    f.context().source(),
                )
            {
                return constant.format().with_options(StringLayout::Flat).fmt(f);
            }
        }

        value.format().with_options(Parenthesize::Optional).fmt(f)
    }
}
