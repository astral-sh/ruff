use crate::prelude::*;
use crate::verbatim_text;
use ruff_formatter::write;
use ruff_python_ast::FStringFormatSpec;

#[derive(Default)]
pub struct FormatFStringFormatSpec;

impl<'ast> FormatNodeRule<'ast, FStringFormatSpec<'ast>> for FormatFStringFormatSpec {
    fn fmt_fields(&self, item: &FStringFormatSpec, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
