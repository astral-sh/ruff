use std::borrow::Cow;

use ruff_python_ast::{PySourceType, Ranged};
use ruff_python_parser::{lexer, AsMode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::TextRange;

#[derive(Debug)]
pub(crate) struct Comment<'a> {
    pub(crate) value: Cow<'a, str>,
    pub(crate) range: TextRange,
}

impl Ranged for Comment<'_> {
    fn range(&self) -> TextRange {
        self.range
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
