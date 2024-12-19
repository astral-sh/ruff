use ruff_python_parser::TokenKind;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use ruff_db::{files::File, parsed::parsed_module, source::source_text};

use crate::{lint::LintId, Db};

#[salsa::tracked(return_ref)]
pub(crate) fn suppressions(db: &dyn Db, file: File) -> Suppressions {
    let source = source_text(db.upcast(), file);
    let parsed = parsed_module(db.upcast(), file);

    // TODO: Support `type: ignore` comments at the
    //   [start of the file](https://typing.readthedocs.io/en/latest/spec/directives.html#type-ignore-comments).
    let mut suppressions = Vec::default();
    let mut line_start = source.bom_start_offset();

    for token in parsed.tokens() {
        match token.kind() {
            TokenKind::Comment => {
                let text = &source[token.range()];

                let suppressed_range = TextRange::new(line_start, token.end());

                if text.strip_prefix("# type: ignore").is_some_and(|suffix| {
                    suffix.is_empty()
                        || suffix.starts_with(char::is_whitespace)
                        || suffix.starts_with('[')
                }) {
                    suppressions.push(Suppression { suppressed_range });
                }
            }
            TokenKind::Newline | TokenKind::NonLogicalNewline => {
                line_start = token.end();
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
    suppressions: Vec<Suppression>,
}

impl Suppressions {
    /// Finds a suppression for the specified lint.
    ///
    /// Returns the first matching suppression if more than one suppression apply to `range` and `id`.
    ///
    /// Returns `None` if the lint isn't suppressed.
    pub(crate) fn find_suppression(&self, range: TextRange, _id: LintId) -> Option<&Suppression> {
        // TODO(micha):
        //   * Test if the suppression suppresses the passed lint
        self.for_range(range).next()
    }

    /// Returns all suppression comments that apply for `range`.
    ///
    /// A suppression applies for the given range if it contains the range's
    /// start or end offset. This means the suppression is on the same line
    /// as the diagnostic's start or end.
    fn for_range(&self, range: TextRange) -> impl Iterator<Item = &Suppression> + '_ {
        // First find the index of the suppression comment that ends right before the range
        // starts. This allows us to skip suppressions that are not relevant for the range.
        let end_offset = self
            .suppressions
            .binary_search_by_key(&range.start(), |suppression| {
                suppression.suppressed_range.end()
            })
            .unwrap_or_else(|index| index);

        // From here, search the remaining suppression comments for one that
        // contains the range's start or end offset. Stop the search
        // as soon as the suppression's range and the range no longer overlap.
        self.suppressions[end_offset..]
            .iter()
            // Stop searching if the suppression starts after the range we're looking for.
            .take_while(move |suppression| range.end() >= suppression.suppressed_range.start())
            .filter(move |suppression| {
                // Don't use intersect to avoid that suppressions on inner-expression
                // ignore errors for outer expressions
                suppression.suppressed_range.contains(range.start())
                    || suppression.suppressed_range.contains(range.end())
            })
    }
}

/// A `type: ignore` or `knot: ignore` suppression comment.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Suppression {
    /// The range for which this suppression applies.
    /// Most of the time, this is the range of the comment's line.
    /// However, there are few cases where the range gets expanded to
    /// cover multiple lines:
    /// * multiline strings: `expr + """multiline\nstring"""  # type: ignore`
    /// * line continuations: `expr \ + "test"  # type: ignore`
    suppressed_range: TextRange,
}
