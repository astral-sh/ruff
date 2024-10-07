use ruff_python_ast::FStringPart;

use crate::other::f_string::FormatFString;
use crate::other::string_literal::StringLiteralKind;
use crate::prelude::*;
use crate::string::Quoting;

/// Formats an f-string part which is either a string literal or an f-string.
///
/// This delegates the actual formatting to the appropriate formatter.
pub(crate) struct FormatFStringPart<'a> {
    part: &'a FStringPart,
    /// The quoting to be used for all the f-string parts. This is determined by
    /// the parent node (f-string expression) and is required to format all parts
    /// correctly.
    quoting: Quoting,
}

impl<'a> FormatFStringPart<'a> {
    pub(crate) fn new(part: &'a FStringPart, quoting: Quoting) -> Self {
        Self { part, quoting }
    }
}

impl Format<PyFormatContext<'_>> for FormatFStringPart<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.part {
            #[allow(deprecated)]
            FStringPart::Literal(string_literal) => string_literal
                .format()
                .with_options(StringLiteralKind::InImplicitlyConcatenatedFString(
                    self.quoting,
                ))
                .fmt(f),
            FStringPart::FString(f_string) => FormatFString::new(f_string, self.quoting).fmt(f),
        }
    }
}
