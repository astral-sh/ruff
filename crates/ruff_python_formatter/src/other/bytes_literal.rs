use ruff_python_ast::BytesLiteral;

use crate::prelude::*;
use crate::string::StringNormalizer;

#[derive(Default)]
pub struct FormatBytesLiteral;

impl FormatNodeRule<ruff_python_ast::kind::BytesLiteral> for FormatBytesLiteral {
    fn fmt_fields(&self, item: BytesLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        StringNormalizer::from_context(f.context())
            .normalize(item.into())
            .fmt(f)
    }
}
