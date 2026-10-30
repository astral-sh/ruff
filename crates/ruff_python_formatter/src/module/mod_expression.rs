use ruff_python_ast::ModExpression;
use ruff_python_ast::flat;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatModExpression;

impl FormatNodeRule<ModExpression> for FormatModExpression {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ModExpression>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let body = item.body();
        body.format().fmt(f)
    }
}
