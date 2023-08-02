use ruff_formatter::write;
use ruff_python_ast::ExprCall;

use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatExprCall;

impl FormatNodeRule<ExprCall> for FormatExprCall {
    fn fmt_fields(&self, item: &ExprCall, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprCall {
            range: _,
            func,
            arguments,
        } = item;

        write!(f, [func.format(), arguments.format()])
    }
}
