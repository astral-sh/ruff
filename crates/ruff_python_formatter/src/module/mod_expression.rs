use ruff_python_ast::ModExpression;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatModExpression;

impl FormatNodeRule<ruff_python_ast::kind::ModExpression> for FormatModExpression {
    fn fmt_fields(&self, item: ModExpression<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let body = item.body();
        body.format().fmt(f)
    }
}
