use ruff_python_ast::FStringPart;

use crate::prelude::*;

/// Formats an f-string part which is either a string literal or an f-string.
///
/// This delegates the actual formatting to the appropriate formatter.
pub(crate) struct FormatFStringPart<'a> {
    part: &'a FStringPart,
}

impl<'a> FormatFStringPart<'a> {
    pub(crate) fn new(part: &'a FStringPart) -> Self {
        Self { part }
    }
}

impl Format<PyFormatContext<'_>> for FormatFStringPart<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.part {
            FStringPart::Literal(string_literal) => string_literal.format().fmt(f),
            FStringPart::FString(f_string) => f_string.format().fmt(f),
        }
    }
}
