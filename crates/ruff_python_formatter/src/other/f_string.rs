use ruff_python_ast::FString;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::preview::is_hex_codes_in_unicode_sequences_enabled;
use crate::string::{Quoting, StringPart};

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

        let result = StringPart::from_source(self.value.range(), &locator)
            .normalize(
                self.quoting,
                &locator,
                f.options().quote_style(),
                f.context().docstring(),
                is_hex_codes_in_unicode_sequences_enabled(f.context()),
            )
            .fmt(f);

        // TODO(dhruvmanila): With PEP 701, comments can be inside f-strings.
        // This is to mark all of those comments as formatted but we need to
        // figure out how to handle them. Note that this needs to be done only
        // after the f-string is formatted, so only for all the non-formatted
        // comments.
        let comments = f.context().comments();
        self.value.elements.iter().for_each(|value| {
            comments.mark_verbatim_node_comments_formatted(value.into());
        });

        result
    }
}
