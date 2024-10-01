use ruff_formatter::write;
use ruff_python_ast::{AnyStringFlags, FString, StringFlags};
use ruff_source_file::Locator;

use crate::prelude::*;
use crate::preview::is_f_string_formatting_enabled;
use crate::string::{Quoting, StringNormalizer, StringQuotes};

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

        let normalizer = StringNormalizer::from_context(f.context())
            .with_quoting(self.quoting)
            .with_preferred_quote_style(f.options().quote_style());

        // If f-string formatting is disabled (not in preview), then we will
        // fall back to the previous behavior of normalizing the f-string.
        if !is_f_string_formatting_enabled(f.context()) {
            let result = normalizer.normalize(self.value.into()).fmt(f);
            let comments = f.context().comments();
            self.value.elements.iter().for_each(|value| {
                comments.mark_verbatim_node_comments_formatted(value.into());
                // Above method doesn't mark the trailing comments of the f-string elements
                // as formatted, so we need to do it manually. For example,
                //
                // ```python
                // f"""foo {
                //     x:.3f
                //     # comment
                // }"""
                // ```
                for trailing_comment in comments.trailing(value) {
                    trailing_comment.mark_formatted();
                }
            });
            return result;
        }

        let string_kind = normalizer.choose_quotes(self.value.into()).flags();

        let context = FStringContext::new(
            string_kind,
            FStringLayout::from_f_string(self.value, &locator),
        );

        // Starting prefix and quote
        let quotes = StringQuotes::from(string_kind);
        write!(f, [string_kind.prefix(), quotes])?;

        f.join()
            .entries(
                self.value
                    .elements
                    .iter()
                    .map(|element| FormatFStringElement::new(element, context)),
            )
            .finish()?;

        // Ending quote
        quotes.fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FStringContext {
    flags: AnyStringFlags,
    layout: FStringLayout,
}

impl FStringContext {
    const fn new(flags: AnyStringFlags, layout: FStringLayout) -> Self {
        Self { flags, layout }
    }

    pub(crate) fn flags(self) -> AnyStringFlags {
        self.flags
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
    fn from_f_string(f_string: &FString, locator: &Locator) -> Self {
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
            .any(|expr| memchr::memchr2(b'\n', b'\r', locator.slice(expr).as_bytes()).is_some())
        {
            Self::Multiline
        } else {
            Self::Flat
        }
    }

    pub(crate) const fn is_multiline(self) -> bool {
        matches!(self, FStringLayout::Multiline)
    }
}
