use ruff_python_ast::StringLiteral;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::preview::is_hex_codes_in_unicode_sequences_enabled;
use crate::string::{docstring, Quoting, StringPart};
use crate::QuoteStyle;

pub(crate) struct FormatStringLiteral<'a> {
    value: &'a StringLiteral,
    layout: StringLiteralKind,
}

impl<'a> FormatStringLiteral<'a> {
    pub(crate) fn new(value: &'a StringLiteral, layout: StringLiteralKind) -> Self {
        Self { value, layout }
    }
}

/// The kind of a string literal.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum StringLiteralKind {
    /// A normal string literal e.g., `"foo"`.
    #[default]
    String,
    /// A string literal used as a docstring.
    Docstring,
    /// A string literal that is implicitly concatenated with an f-string. This
    /// makes the overall expression an f-string whose quoting detection comes
    /// from the parent node (f-string expression).
    InImplicitlyConcatenatedFString(Quoting),
}

impl StringLiteralKind {
    /// Checks if this string literal is a docstring.
    pub(crate) const fn is_docstring(self) -> bool {
        matches!(self, StringLiteralKind::Docstring)
    }

    /// Returns the quoting to be used for this string literal.
    fn quoting(self) -> Quoting {
        match self {
            StringLiteralKind::String | StringLiteralKind::Docstring => Quoting::CanChange,
            StringLiteralKind::InImplicitlyConcatenatedFString(quoting) => quoting,
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringLiteral<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();

        let quote_style = if self.layout.is_docstring() {
            // Per PEP 8 and PEP 257, always prefer double quotes for docstrings
            QuoteStyle::Double
        } else {
            f.options().quote_style()
        };

        let normalized = StringPart::from_source(self.value.range(), &locator).normalize(
            self.layout.quoting(),
            &locator,
            quote_style,
            f.context().docstring(),
            is_hex_codes_in_unicode_sequences_enabled(f.context()),
        );

        if self.layout.is_docstring() {
            docstring::format(&normalized, f)
        } else {
            normalized.fmt(f)
        }
    }
}
