use crate::expression::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;
use rustpython_parser::ast::StmtExpr;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtExpr { value, .. } = item;

        value.format().with_options(Parenthesize::Optional).fmt(f)
    }
}
