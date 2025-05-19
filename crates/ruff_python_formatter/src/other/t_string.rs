use super::f_t_string_element::FormatFTStringElement;
use crate::other::f_t_string::{FTStringContext, FTStringLayout};
use crate::prelude::*;
use crate::string::{StringNormalizer, StringQuotes};
use ruff_formatter::write;
use ruff_python_ast::{StringFlags, TString};

/// Formats an f-string which is part of a larger f-string expression.
///
/// For example, this would be used to format the f-string part in `"foo" f"bar {x}"`
/// or the standalone f-string in `f"foo {x} bar"`.
#[derive(Default)]
pub struct FormatTString;

impl FormatNodeRule<TString> for FormatTString {
    fn fmt_fields(&self, item: &TString, f: &mut PyFormatter) -> FormatResult<()> {
        let normalizer = StringNormalizer::from_context(f.context());

        let string_kind = normalizer.choose_quotes(item.into()).flags();

        let context = FTStringContext::new(
            string_kind,
            FTStringLayout::from_t_string(item, f.context().source()),
        );

        // Starting prefix and quote
        let quotes = StringQuotes::from(string_kind);
        write!(f, [string_kind.prefix(), quotes])?;

        for element in &item.elements {
            FormatFTStringElement::new(element, context).fmt(f)?;
        }

        // Ending quote
        quotes.fmt(f)
    }
}
