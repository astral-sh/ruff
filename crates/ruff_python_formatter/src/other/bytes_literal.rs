use ruff_python_ast::BytesLiteral;
use ruff_python_ast::flat;

use crate::prelude::*;
use crate::string::StringNormalizer;

#[derive(Default)]
pub struct FormatBytesLiteral;

impl FormatNodeRule<BytesLiteral> for FormatBytesLiteral {
    fn fmt_fields(
        &self,
        item: flat::View<'_, BytesLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        StringNormalizer::from_context(f.context())
            .normalize(item.into())
            .fmt(f)
    }
}
