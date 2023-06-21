use crate::prelude::*;
use crate::AsFormat;
use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use rustpython_parser::ast::{Identifier, Ranged};

pub struct FormatIdentifier;

impl FormatRule<Identifier, PyFormatContext<'_>> for FormatIdentifier {
    fn fmt(&self, item: &Identifier, f: &mut PyFormatter) -> FormatResult<()> {
        source_text_slice(item.range(), ContainsNewlines::No).fmt(f)
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Identifier {
    type Format<'a> = FormatRefWithRule<'a, Identifier, FormatIdentifier, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatIdentifier)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Identifier {
    type Format = FormatOwnedWithRule<Identifier, FormatIdentifier, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatIdentifier)
    }
}
