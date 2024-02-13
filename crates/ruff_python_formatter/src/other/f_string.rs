use ruff_formatter::write;
use ruff_python_ast::FString;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::preview::is_pep_701_enabled;
use crate::string::{Quoting, StringNormalizer, StringPart, StringPrefix, StringQuotes};

use super::f_string_element::FormatFStringElement;

/// Formats an f-string which is part of a larger f-string expression.
///
/// For example, this would be used to format the f-string part in `"foo" f"bar {x}"`
/// or the standalone f-string in `f"foo {x} bar"`.
pub(crate) struct FormatFString<'a> {
    value: &'a FString,
    /// The quoting of an f-string. This is determined by the parent node
    /// (f-string expression) and is required to format an f-string correctly.
    quoting: Quoting,
}

impl<'a> FormatFString<'a> {
    pub(crate) fn new(value: &'a FString, quoting: Quoting) -> Self {
        Self { value, quoting }
    }
}

impl Format<PyFormatContext<'_>> for FormatFString<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();
        let comments = f.context().comments().clone();

        let normalizer = StringNormalizer::from_context(f.context())
            .with_quoting(self.quoting)
            .with_preferred_quote_style(f.options().quote_style());

        if !is_pep_701_enabled(f.context()) {
            let result = normalizer
                .normalize(
                    &StringPart::from_source(self.value.range(), &locator),
                    &locator,
                )
                .fmt(f);
            self.value.elements.iter().for_each(|value| {
                comments.mark_verbatim_node_comments_formatted(value.into());
            });
            return result;
        }

        let string = StringPart::from_source(self.value.range(), &locator);

        // TODO(dhruvmanila): This could probably be simplified for Python 3.12 specifically
        // as same quotes can be re-used inside an f-string.
        let quotes = normalizer.choose_quotes(&string, &locator);

        let is_multiline =
            memchr::memchr2(b'\n', b'\r', locator.slice(self.value).as_bytes()).is_some();
        let context = FStringContext::new(string.prefix(), quotes, is_multiline);

        // Starting prefix and quote
        write!(f, [string.prefix(), quotes])?;

        format_with(|f| {
            f.join()
                .entries(
                    self.value
                        .elements
                        .iter()
                        .map(|element| FormatFStringElement::new(element, context)),
                )
                .finish()
        })
        .fmt(f)?;

        // Ending quote
        quotes.fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FStringContext {
    prefix: StringPrefix,
    quotes: StringQuotes,
    is_multiline: bool,
}

impl FStringContext {
    const fn new(prefix: StringPrefix, quotes: StringQuotes, is_multiline: bool) -> Self {
        Self {
            prefix,
            quotes,
            is_multiline,
        }
    }

    pub(crate) const fn quotes(self) -> StringQuotes {
        self.quotes
    }

    pub(crate) const fn prefix(self) -> StringPrefix {
        self.prefix
    }

    pub(crate) const fn should_remove_soft_line_breaks(self) -> bool {
        !self.quotes.is_triple() && !self.is_multiline
    }
}
