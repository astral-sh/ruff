use ruff_python_ast::BytesLiteral;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::preview::is_hex_codes_in_unicode_sequences_enabled;
use crate::string::{Quoting, StringPart};

#[derive(Default)]
pub struct FormatBytesLiteral;

impl FormatNodeRule<BytesLiteral> for FormatBytesLiteral {
    fn fmt_fields(&self, item: &BytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();

        StringPart::from_source(item.range(), &locator)
            .normalize(
                Quoting::CanChange,
                &locator,
                f.options().quote_style(),
                f.context().docstring(),
                is_hex_codes_in_unicode_sequences_enabled(f.context()),
            )
            .fmt(f)
    }
}
