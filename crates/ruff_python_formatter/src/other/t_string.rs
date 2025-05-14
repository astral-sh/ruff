use ruff_formatter::write;
use ruff_python_ast::{AnyStringFlags, StringFlags, TString};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::{StringNormalizer, StringQuotes};

use super::t_string_element::FormatTStringElement;

/// Formats a t-string which is part of a larger t-string expression.
///
/// For example, this would be used to format the t-string part in `"foo" t"bar {x}"`
/// or the standalone t-string in `t"foo {x} bar"`.
#[derive(Default)]
pub struct FormatTString;

impl FormatNodeRule<TString> for FormatTString {
    fn fmt_fields(&self, item: &TString, f: &mut PyFormatter) -> FormatResult<()> {
        let normalizer = StringNormalizer::from_context(f.context());

        let string_kind = normalizer.choose_quotes(item.into()).flags();

        let context = TStringContext::new(
            string_kind,
            TStringLayout::from_t_string(item, f.context().source()),
        );

        // Starting prefix and quote
        let quotes = StringQuotes::from(string_kind);
        write!(f, [string_kind.prefix(), quotes])?;

        for element in &item.elements {
            FormatTStringElement::new(element, context).fmt(f)?;
        }

        // Ending quote
        quotes.fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TStringContext {
    /// The string flags of the enclosing t-string part.
    enclosing_flags: AnyStringFlags,
    layout: TStringLayout,
}

impl TStringContext {
    pub(crate) const fn new(flags: AnyStringFlags, layout: TStringLayout) -> Self {
        Self {
            enclosing_flags: flags,
            layout,
        }
    }

    pub(crate) fn flags(self) -> AnyStringFlags {
        self.enclosing_flags
    }

    pub(crate) const fn layout(self) -> TStringLayout {
        self.layout
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum TStringLayout {
    /// Original t-string is flat.
    /// Don't break expressions to keep the string flat.
    Flat,
    /// Original t-string has multiline expressions in the replacement fields.
    /// Allow breaking expressions across multiple lines.
    Multiline,
}

impl TStringLayout {
    pub(crate) fn from_t_string(t_string: &TString, source: &str) -> Self {
        // Heuristic: Allow breaking the t-string expressions across multiple lines
        // only if there already is at least one multiline expression. This puts the
        // control in the hands of the user to decide if they want to break the
        // t-string expressions across multiple lines or not. This is similar to
        // how Prettier does it for template literals in JavaScript.
        //
        // Reference: https://prettier.io/docs/en/next/rationale.html#template-literals
        if t_string
            .elements
            .interpolations()
            .any(|expr| source.contains_line_break(expr.range()))
        {
            Self::Multiline
        } else {
            Self::Flat
        }
    }

    pub(crate) const fn is_flat(self) -> bool {
        matches!(self, TStringLayout::Flat)
    }

    pub(crate) const fn is_multiline(self) -> bool {
        matches!(self, TStringLayout::Multiline)
    }
}
