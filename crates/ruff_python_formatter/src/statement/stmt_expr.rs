use rustpython_parser::ast::StmtExpr;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtExpr { value, .. } = item;

        maybe_parenthesize_expression(value, item, Parenthesize::Optional).fmt(f)
    }
}
