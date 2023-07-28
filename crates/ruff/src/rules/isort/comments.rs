use std::borrow::Cow;

use ruff_python_parser::{lexer, Tok};
use ruff_text_size::{TextRange, TextSize};

use ruff_source_file::Locator;

use crate::source_kind::PySourceType;

#[derive(Debug)]
pub(crate) struct Comment<'a> {
    pub(crate) value: Cow<'a, str>,
    pub(crate) range: TextRange,
}

impl Comment<'_> {
    pub(crate) const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub(crate) const fn end(&self) -> TextSize {
        self.range.end()
    }
}

/// Collect all comments in an import block.
pub(crate) fn collect_comments<'a>(
    range: TextRange,
    locator: &'a Locator,
    source_type: PySourceType,
) -> Vec<Comment<'a>> {
    let contents = locator.slice(range);
    lexer::lex_starts_at(contents, source_type.as_mode(), range.start())
        .flatten()
        .filter_map(|(tok, range)| {
            if let Tok::Comment(value) = tok {
                Some(Comment {
                    value: value.into(),
                    range,
                })
            } else {
                None
            }
        })
        .collect()
}
