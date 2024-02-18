use ruff_python_ast::BytesLiteral;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::{StringNormalizer, StringPart};

#[derive(Default)]
pub struct FormatBytesLiteral;

impl FormatNodeRule<BytesLiteral> for FormatBytesLiteral {
    fn fmt_fields(&self, item: &BytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();

        StringNormalizer::from_context(f.context())
            .with_preferred_quote_style(f.options().quote_style())
            .normalize(&StringPart::from_source(item.range(), &locator), &locator)
            .fmt(f)
    }
}
