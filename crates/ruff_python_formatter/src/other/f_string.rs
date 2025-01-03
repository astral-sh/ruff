use ruff_formatter::write;
use ruff_python_ast::{AnyStringFlags, FString, StringFlags};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::{StringNormalizer, StringQuotes};

use super::f_string_element::FormatFStringElement;

/// Formats an f-string which is part of a larger f-string expression.
///
/// For example, this would be used to format the f-string part in `"foo" f"bar {x}"`
/// or the standalone f-string in `f"foo {x} bar"`.
#[derive(Default)]
pub struct FormatFString;

impl FormatNodeRule<FString> for FormatFString {
    fn fmt_fields(&self, item: &FString, f: &mut PyFormatter) -> FormatResult<()> {
        let normalizer = StringNormalizer::from_context(f.context());

        let string_kind = normalizer.choose_quotes(item.into()).flags();

        let context = FStringContext::new(
            string_kind,
            FStringLayout::from_f_string(item, f.context().source()),
        );

        // Starting prefix and quote
        let quotes = StringQuotes::from(string_kind);
        write!(f, [string_kind.prefix(), quotes])?;

        for element in &item.elements {
            FormatFStringElement::new(element, context).fmt(f)?;
        }

        // Ending quote
        quotes.fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FStringContext {
    /// The string flags of the enclosing f-string part.
    enclosing_flags: AnyStringFlags,
    layout: FStringLayout,
}

impl FStringContext {
    pub(crate) const fn new(flags: AnyStringFlags, layout: FStringLayout) -> Self {
        Self {
            enclosing_flags: flags,
            layout,
        }
    }

    pub(crate) fn flags(self) -> AnyStringFlags {
        self.enclosing_flags
    }

    pub(crate) const fn layout(self) -> FStringLayout {
        self.layout
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum FStringLayout {
    /// Original f-string is flat.
    /// Don't break expressions to keep the string flat.
    Flat,
    /// Original f-string has multiline expressions in the replacement fields.
    /// Allow breaking expressions across multiple lines.
    Multiline,
}

impl FStringLayout {
    pub(crate) fn from_f_string(f_string: &FString, source: &str) -> Self {
        // Heuristic: Allow breaking the f-string expressions across multiple lines
        // only if there already is at least one multiline expression. This puts the
        // control in the hands of the user to decide if they want to break the
        // f-string expressions across multiple lines or not. This is similar to
        // how Prettier does it for template literals in JavaScript.
        //
        // If it's single quoted f-string and it contains a multiline expression, then we
        // assume that the target version of Python supports it (3.12+). If there are comments
        // used in any of the expression of the f-string, then it's always going to be multiline
        // and we assume that the target version of Python supports it (3.12+).
        //
        // Reference: https://prettier.io/docs/en/next/rationale.html#template-literals
        if f_string
            .elements
            .expressions()
            .any(|expr| source.contains_line_break(expr.range()))
        {
            Self::Multiline
        } else {
            Self::Flat
        }
    }

    pub(crate) const fn is_flat(self) -> bool {
        matches!(self, FStringLayout::Flat)
    }

    pub(crate) const fn is_multiline(self) -> bool {
        matches!(self, FStringLayout::Multiline)
    }
}
