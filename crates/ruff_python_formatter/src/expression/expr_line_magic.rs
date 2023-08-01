use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::ExprLineMagic;

#[derive(Default)]
pub struct FormatExprLineMagic;

impl FormatNodeRule<ExprLineMagic> for FormatExprLineMagic {
    fn fmt_fields(&self, item: &ExprLineMagic, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
