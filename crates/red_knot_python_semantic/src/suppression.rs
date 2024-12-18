use std::cmp::Ordering;

use ruff_python_parser::TokenKind;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

use ruff_db::{files::File, parsed::parsed_module, source::source_text};
use ruff_index::{newtype_index, IndexVec};

use crate::{lint::LintId, Db};

#[salsa::tracked(return_ref)]
pub(crate) fn suppressions(db: &dyn Db, file: File) -> Suppressions {
    let source = source_text(db.upcast(), file);
    let parsed = parsed_module(db.upcast(), file);

    let mut suppressions = IndexVec::default();
    let mut line_start = source.bom_start_offset();

    for token in parsed.tokens() {
        match token.kind() {
            TokenKind::Comment => {
                let text = &source[token.range()];

                let suppressed_range = TextRange::new(line_start, token.end());

                if text.strip_prefix("# type: ignore").is_some_and(|suffix| {
                    suffix.is_empty() || suffix.starts_with(char::is_whitespace)
                }) {
                    suppressions.push(Suppression { suppressed_range });
                }
            }
            TokenKind::Newline | TokenKind::NonLogicalNewline => {
                line_start = token.range().end();
            }
            _ => {}
        }
    }

    Suppressions { suppressions }
}

/// The suppression comments of a single file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Suppressions {
    /// The suppressions sorted by the suppressed range.
    suppressions: IndexVec<SuppressionIndex, Suppression>,
}

impl Suppressions {
    pub(crate) fn find_suppression(
        &self,
        range: TextRange,
        _id: LintId,
    ) -> Option<SuppressionIndex> {
        let enclosing_index = self.enclosing_suppression(range.end())?;

        // TODO(micha):
        //   * Test if the suppression suppresses the passed lint

        Some(enclosing_index)
    }

    fn enclosing_suppression(&self, offset: TextSize) -> Option<SuppressionIndex> {
        self.suppressions
            .binary_search_by(|suppression| {
                if suppression.suppressed_range.contains(offset) {
                    Ordering::Equal
                } else if suppression.suppressed_range.end() < offset {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            })
            .ok()
    }
}

impl std::ops::Index<SuppressionIndex> for Suppressions {
    type Output = Suppression;

    fn index(&self, index: SuppressionIndex) -> &Self::Output {
        &self.suppressions[index]
    }
}

#[newtype_index]
pub(crate) struct SuppressionIndex;

/// A `type: ignore` or `knot: ignore` suppression comment.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Suppression {
    /// The range for which this suppression applies.
    /// Most of the time, this is the range of the comment's line.
    /// However, there are few cases where the range gets expanted to
    /// cover multiple lines:
    /// * multiline strings: `expr + """multiline\nstring"""  # type: ignore`
    /// * line continuations: `expr \ + "test"  # type: ignore`
    suppressed_range: TextRange,
}
