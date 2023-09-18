use ruff_python_ast::ModExpression;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatModExpression;

impl FormatNodeRule<ModExpression> for FormatModExpression {
    fn fmt_fields(&self, item: &ModExpression, f: &mut PyFormatter) -> FormatResult<()> {
        let ModExpression { body, range: _ } = item;
        body.format().fmt(f)
    }
}
