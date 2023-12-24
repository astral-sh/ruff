use ruff_python_ast::BytesLiteral;
use ruff_text_size::Ranged;

use crate::prelude::*;
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
            )
            .fmt(f)
    }
}
