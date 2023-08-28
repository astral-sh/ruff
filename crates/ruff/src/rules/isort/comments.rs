use std::borrow::Cow;

use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

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
    indexer: &'a Indexer,
) -> Vec<Comment<'a>> {
    indexer
        .comment_ranges()
        .comments_in_range(range)
        .iter()
        .map(|range| Comment {
            value: locator.slice(*range).into(),
            range: *range,
        })
        .collect()
}
