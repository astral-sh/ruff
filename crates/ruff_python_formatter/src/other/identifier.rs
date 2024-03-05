use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::Identifier;
use ruff_python_trivia::is_python_whitespace;
use ruff_text_size::Ranged;

use crate::prelude::*;

pub struct FormatIdentifier;

impl FormatRule<Identifier, PyFormatContext<'_>> for FormatIdentifier {
    fn fmt(&self, item: &Identifier, f: &mut PyFormatter) -> FormatResult<()> {
        source_text_slice(item.range()).fmt(f)
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

/// A formatter for a dot-delimited identifier, as seen in import statements:
/// ```python
/// import foo.bar
/// from tqdm   . auto import tqdm
/// ```
///
/// Dot-delimited identifiers can contain newlines via continuations (backslashes) after the
/// dot-delimited segment, as in:
/// ```python
/// import foo\
///    .bar
/// ```
///
/// While identifiers can typically be formatted via verbatim source code slices, dot-delimited
/// identifiers with newlines must be formatted via `text`. This struct implements both the fast
/// and slow paths for such identifiers.
#[derive(Debug)]
pub(crate) struct DotDelimitedIdentifier<'a>(&'a Identifier);

impl<'a> DotDelimitedIdentifier<'a> {
    pub(crate) fn new(identifier: &'a Identifier) -> Self {
        Self(identifier)
    }
}

impl Format<PyFormatContext<'_>> for DotDelimitedIdentifier<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        // An import identifier can contain whitespace around the dots:
        // ```python
        // import importlib   .   metadata
        // ```
        // It can also contain newlines by inserting continuations (backslashes) after
        // a dot-delimited segment, as in:
        // ```python
        // import foo\
        //     .bar
        // ```
        if f.context().source()[self.0.range()]
            .chars()
            .any(|c| is_python_whitespace(c) || matches!(c, '\n' | '\r' | '\\'))
        {
            let no_whitespace: String = f.context().source()[self.0.range()]
                .chars()
                .filter(|c| !is_python_whitespace(*c) && !matches!(c, '\n' | '\r' | '\\'))
                .collect();
            text(&no_whitespace).fmt(f)
        } else {
            source_text_slice(self.0.range()).fmt(f)
        }
    }
}
