use crate::{Printed, SourceMarker, TextRange};
use ruff_rowan::TextLen;
use ruff_rowan::{Language, SyntaxNode, TextSize};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::iter::FusedIterator;

/// A source map for mapping positions of a pre-processed tree back to the locations in the source tree.
///
/// This is not a generic purpose source map but instead focused on supporting the case where
/// a language removes or re-orders nodes that would otherwise complicate the formatting logic.
/// A common use case for pre-processing is the removal of all parenthesized nodes.
/// Removing parenthesized nodes simplifies the formatting logic when it has different behaviour
/// depending if a child or parent is of a specific node kind. Performing such a test with parenthesized
/// nodes present in the source code means that the formatting logic has to skip over all parenthesized nodes
/// until it finds the first non-parenthesized node and then test if that node is of the expected kind.
///
/// This source map implementation supports removing tokens or re-structuring nodes
/// without changing the order of the tokens in the tree (requires no source map).
///
/// The following section uses parentheses as a concrete example to explain the functionality of the source map.
/// However, the source map implementation isn't restricted to removing parentheses only, it supports mapping
/// transformed to source position for any use case where a transform deletes text from the source tree.
///
/// ## Position Mapping
///
/// The source map internally tracks all the ranges that have been deleted from the source code sorted by the start of the deleted range.
/// It further stores the absolute count of deleted bytes preceding a range. The deleted range together
/// with the absolute count allows to re-compute the source location for every transformed location
/// and has the benefit that it requires significantly fewer memory
/// than source maps that use a source to destination position marker for every token.
///
/// ## Map Node Ranges
///
/// Only having the deleted ranges to resolve the original text of a node isn't sufficient.
/// Resolving the original text of a node is needed when formatting a node as verbatim, either because
/// formatting the node failed because of a syntax error, or formatting is suppressed with a `rome-ignore format:` comment.
///
/// ```text
/// // Source           // Transformed
///  (a+b) + (c + d)   a + b + c + d;
/// ```
///
/// Using the above example, the following source ranges should be returned when querying with the transformed ranges:
///
/// * `a` -> `a`: Should not include the leading `(`
/// * `b` -> `b`: Should not include the trailing `)`
/// * `a + b` -> `(a + b)`: Should include the leading `(` and trailing `)`.
/// * `a + b + c + d` -> `(a + b) + (c + d)`: Should include the fist `(` token and the last `)` token because the expression statement
///   fully encloses the `a + b` and `c + d` nodes.
///
/// This is why the source map also tracks the mapped trimmed ranges for every node.
#[derive(Debug, Clone)]
pub struct TransformSourceMap {
    source_text: String,

    /// The mappings stored in increasing order
    deleted_ranges: Vec<DeletedRange>,

    /// Key: Start or end position of node for which the trimmed range should be extended
    /// Value: The trimmed range.
    mapped_node_ranges: FxHashMap<TextSize, TrimmedNodeRangeMapping>,
}

impl TransformSourceMap {
    /// Returns the text of the source document as it was before the transformation.
    pub fn text(&self) -> &str {
        &self.source_text
    }

    /// Maps a range of the transformed document to a range in the source document.
    ///
    /// Complexity: `O(log(n))`
    pub fn source_range(&self, transformed_range: TextRange) -> TextRange {
        let range = TextRange::new(
            self.source_offset(transformed_range.start(), RangePosition::Start),
            self.source_offset(transformed_range.end(), RangePosition::End),
        );

        debug_assert!(range.end() <= self.source_text.text_len(), "Mapped range {:?} exceeds the length of the source document {:?}. Please check if the passed `transformed_range` is a range of the transformed tree and not of the source tree, and that it belongs to the tree for which the source map was created for.", range, self.source_text.len());
        range
    }

    /// Maps the trimmed range of the transformed node to the trimmed range in the source document.
    ///
    /// Average Complexity: `O(log(n))`
    pub fn trimmed_source_range<L: Language>(&self, node: &SyntaxNode<L>) -> TextRange {
        self.trimmed_source_range_from_transformed_range(node.text_trimmed_range())
    }

    fn resolve_trimmed_range(&self, mut source_range: TextRange) -> TextRange {
        let start_mapping = self.mapped_node_ranges.get(&source_range.start());
        if let Some(mapping) = start_mapping {
            // If the queried node fully encloses the original range of the node, then extend the range
            if source_range.contains_range(mapping.original_range) {
                source_range = TextRange::new(mapping.extended_range.start(), source_range.end());
            }
        }

        let end_mapping = self.mapped_node_ranges.get(&source_range.end());
        if let Some(mapping) = end_mapping {
            // If the queried node fully encloses the original range of the node, then extend the range
            if source_range.contains_range(mapping.original_range) {
                source_range = TextRange::new(source_range.start(), mapping.extended_range.end());
            }
        }

        source_range
    }

    fn trimmed_source_range_from_transformed_range(
        &self,
        transformed_range: TextRange,
    ) -> TextRange {
        let source_range = self.source_range(transformed_range);

        let mut mapped_range = source_range;

        loop {
            let resolved = self.resolve_trimmed_range(mapped_range);

            if resolved == mapped_range {
                break resolved;
            } else {
                mapped_range = resolved;
            }
        }
    }

    /// Returns the source text of the trimmed range of `node`.
    pub fn trimmed_source_text<L: Language>(&self, node: &SyntaxNode<L>) -> &str {
        let range = self.trimmed_source_range(node);
        &self.source_text[range]
    }

    /// Returns an iterator over all deleted ranges in increasing order by their start position.
    pub fn deleted_ranges(&self) -> DeletedRanges {
        DeletedRanges {
            source_text: &self.source_text,
            deleted_ranges: self.deleted_ranges.iter(),
        }
    }

    #[cfg(test)]
    fn trimmed_source_text_from_transformed_range(&self, range: TextRange) -> &str {
        let range = self.trimmed_source_range_from_transformed_range(range);
        &self.source_text[range]
    }

    fn source_offset(&self, transformed_offset: TextSize, position: RangePosition) -> TextSize {
        let index = self
            .deleted_ranges
            .binary_search_by_key(&transformed_offset, |range| range.transformed_start());

        let range = match index {
            Ok(index) => Some(&self.deleted_ranges[index]),
            Err(index) => {
                if index == 0 {
                    None
                } else {
                    self.deleted_ranges.get(index - 1)
                }
            }
        };

        self.source_offset_with_range(transformed_offset, position, range)
    }

    fn source_offset_with_range(
        &self,
        transformed_offset: TextSize,
        position: RangePosition,
        deleted_range: Option<&DeletedRange>,
    ) -> TextSize {
        match deleted_range {
            Some(range) => {
                debug_assert!(
                    range.transformed_start() <= transformed_offset,
                    "Transformed start {:?} must be less than or equal to transformed offset {:?}.",
                    range.transformed_start(),
                    transformed_offset
                );
                // Transformed position directly falls onto a position where a deleted range starts or ends (depending on the position)
                // For example when querying: `a` in `(a)` or (a + b)`, or `b`
                if range.transformed_start() == transformed_offset {
                    match position {
                        RangePosition::Start => range.source_end(),
                        // `a)`, deleted range is right after the token. That's why `source_start` is the offset
                        // that truncates the `)` and `source_end` includes it
                        RangePosition::End => range.source_start(),
                    }
                }
                // The position falls outside of a position that has a leading/trailing deleted range.
                // For example, if you get the position of `+` in `(a + b)`.
                // That means, the trimmed and non-trimmed offsets are the same
                else {
                    let transformed_delta = transformed_offset - range.transformed_start();
                    range.source_start() + range.len() + transformed_delta
                }
            }
            None => transformed_offset,
        }
    }

    /// Maps the source code positions relative to the transformed tree of `printed` to the location
    /// in the original, untransformed source code.
    ///
    /// The printer creates a source map that allows mapping positions from the newly formatted document
    /// back to the locations of the tree. However, the source positions stored in [crate::FormatElement::DynamicText]
    /// and [crate::FormatElement::SyntaxTokenTextSlice] are relative to the transformed tree
    /// and not the original tree passed to [crate::format_node].
    ///
    /// This function re-maps the positions from the positions in the transformed tree back to the positions
    /// in the original, untransformed tree.
    pub fn map_printed(&self, mut printed: Printed) -> Printed {
        self.map_markers(&mut printed.sourcemap);

        printed
    }

    /// Maps the printers source map marker to the source positions.
    fn map_markers(&self, markers: &mut [SourceMarker]) {
        if self.deleted_ranges.is_empty() {
            return;
        }

        let mut previous_marker: Option<SourceMarker> = None;
        let mut next_range_index = 0;

        for marker in markers {
            // It's not guaranteed that markers are sorted by source location (line suffix comments).
            // It can, therefore, be necessary to navigate backwards again.
            // In this case, do a binary search for the index of the next deleted range (`O(log(n)`).
            let out_of_order_marker =
                previous_marker.map_or(false, |previous| previous.source > marker.source);

            if out_of_order_marker {
                let index = self
                    .deleted_ranges
                    .binary_search_by_key(&marker.source, |range| range.transformed_start());

                match index {
                    // Direct match
                    Ok(index) => {
                        next_range_index = index + 1;
                    }
                    Err(index) => next_range_index = index,
                }
            } else {
                // Find the range for this mapping. In most cases this is a no-op or only involves a single step
                // because markers are most of the time in increasing source order.
                while next_range_index < self.deleted_ranges.len() {
                    let next_range = &self.deleted_ranges[next_range_index];

                    if next_range.transformed_start() > marker.source {
                        break;
                    }

                    next_range_index += 1;
                }
            }

            previous_marker = Some(*marker);

            let current_range = if next_range_index == 0 {
                None
            } else {
                self.deleted_ranges.get(next_range_index - 1)
            };

            let source =
                self.source_offset_with_range(marker.source, RangePosition::Start, current_range);

            marker.source = source;
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct TrimmedNodeRangeMapping {
    /// The original trimmed range of the node.
    ///
    /// ```javascript
    /// (a + b)
    /// ```
    ///
    /// `1..6` `a + b`
    original_range: TextRange,

    /// The range to which the trimmed range of the node should be extended
    /// ```javascript
    /// (a + b)
    /// ```
    ///
    /// `0..7` for `a + b` if its range should also include the parenthesized range.
    extended_range: TextRange,
}

#[derive(Copy, Clone, Debug)]
enum RangePosition {
    Start,
    End,
}

/// Stores the information about a range in the source document that isn't present in the transformed document
/// and provides means to map the transformed position back to the source position.
///
/// # Examples
///
/// ```javascript
/// (a + b)
/// ```
///
/// A transform that removes the parentheses from the above expression removes the ranges `0..1` (`(` token)
/// and `6..7` (`)` token) and the source map creates one [DeletedRange] for each:
///
/// ```text
/// DeletedRange {
///     source_range: 0..1,
///     total_length_preceding_deleted_ranges: 0,
/// },
/// DeletedRange {
///     source_range: 6..7,
///     total_length_preceding_deleted_ranges: 1,
/// }
/// ```
///
/// The first range indicates that the range `0..1` for the `(` token has been removed. The second range
/// indicates that the range `6..7` for the `)` token has been removed and it stores that, up to this point,
/// but not including, 1 more byte has been removed.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct DeletedRange {
    /// The range in the source document of the bytes that have been omitted from the transformed document.
    source_range: TextRange,

    /// The accumulated count of all removed bytes up to (but not including) the start of this range.
    total_length_preceding_deleted_ranges: TextSize,
}

impl DeletedRange {
    fn new(source_range: TextRange, total_length_preceding_deleted_ranges: TextSize) -> Self {
        debug_assert!(source_range.start() >= total_length_preceding_deleted_ranges, "The total number of deleted bytes ({:?}) can not exceed the offset from the start in the source document ({:?}). This is a bug in the source map implementation.", total_length_preceding_deleted_ranges, source_range.start());

        Self {
            source_range,
            total_length_preceding_deleted_ranges,
        }
    }

    /// The number of deleted characters starting from [source offset](DeletedRange::source_start).
    fn len(&self) -> TextSize {
        self.source_range.len()
    }

    /// The start position in bytes in the source document of the omitted sequence in the transformed document.
    fn source_start(&self) -> TextSize {
        self.source_range.start()
    }

    /// The end position in bytes in the source document of the omitted sequence in the transformed document.
    fn source_end(&self) -> TextSize {
        self.source_range.end()
    }

    /// Returns the byte position of [DeleteRange::source_start] in the transformed document.
    fn transformed_start(&self) -> TextSize {
        self.source_range.start() - self.total_length_preceding_deleted_ranges
    }
}

/// Builder for creating a source map.
#[derive(Debug, Default)]
pub struct TransformSourceMapBuilder {
    /// The original source text of the tree before it was transformed.
    source_text: String,

    /// The mappings in increasing order by transformed offset.
    deleted_ranges: Vec<TextRange>,

    /// The keys are a position in the source map where a trimmed node starts or ends.
    /// The values are the metadata about a trimmed node range
    mapped_node_ranges: FxHashMap<TextSize, TrimmedNodeRangeMapping>,
}

impl TransformSourceMapBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    /// Creates a new builder for a document with the given source.
    pub fn with_source(source: String) -> Self {
        Self {
            source_text: source,
            ..Default::default()
        }
    }

    /// Appends `text` to the source text of the original document.
    pub fn push_source_text(&mut self, text: &str) {
        self.source_text.push_str(text);
    }

    /// Adds a new mapping for a deleted character range.
    pub fn add_deleted_range(&mut self, source_range: TextRange) {
        self.deleted_ranges.push(source_range);
    }

    /// Adds a mapping to widen a nodes trimmed range.
    ///
    /// The formatter uses the trimmed range when formatting a node in verbatim either because the node
    /// failed to format because of a syntax error or because it's formatting is suppressed with a `rome-ignore format:` comment.
    ///
    /// This method adds a mapping to widen a nodes trimmed range to enclose another range instead. This is
    /// e.g. useful when removing parentheses around expressions where `(/* comment */ a /* comment */)` because
    /// the trimmed range of `a` should now enclose the full range including the `(` and `)` tokens to ensure
    /// that the parentheses are retained when printing that node in verbatim style.
    pub fn extend_trimmed_node_range(
        &mut self,
        original_range: TextRange,
        extended_range: TextRange,
    ) {
        let mapping = TrimmedNodeRangeMapping {
            original_range,
            extended_range,
        };

        self.mapped_node_ranges
            .insert(original_range.start(), mapping);
        self.mapped_node_ranges
            .insert(original_range.end(), mapping);
    }

    /// Creates a source map that performs single position lookups in `O(log(n))`.
    pub fn finish(mut self) -> TransformSourceMap {
        let mut merged_mappings = Vec::with_capacity(self.deleted_ranges.len());

        if !self.deleted_ranges.is_empty() {
            self.deleted_ranges
                .sort_by(|a, b| match a.start().cmp(&b.start()) {
                    Ordering::Equal => a.end().cmp(&b.end()),
                    ordering => ordering,
                });

            let mut last_mapping = DeletedRange::new(
                // SAFETY: Safe because of the not empty check above
                self.deleted_ranges[0],
                TextSize::default(),
            );

            let mut transformed_offset = last_mapping.len();

            for range in self.deleted_ranges.drain(1..) {
                // Merge adjacent ranges to ensure there's only ever a single mapping starting at the same transformed offset.
                if last_mapping.source_range.end() == range.start() {
                    last_mapping.source_range = last_mapping.source_range.cover(range);
                } else {
                    merged_mappings.push(last_mapping);

                    last_mapping = DeletedRange::new(range, transformed_offset);
                }
                transformed_offset += range.len();
            }

            merged_mappings.push(last_mapping);
        }

        TransformSourceMap {
            source_text: self.source_text,
            deleted_ranges: merged_mappings,
            mapped_node_ranges: self.mapped_node_ranges,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeletedRangeEntry<'a> {
    /// The start position of the removed range in the source document
    pub source: TextSize,

    /// The position in the transformed document where the removed range would have been (but is not, because it was removed)
    pub transformed: TextSize,

    /// The text of the removed range
    pub text: &'a str,
}

/// Iterator over all removed ranges in a document.
///
/// Returns the ranges in increased order by their start position.
pub struct DeletedRanges<'a> {
    source_text: &'a str,

    /// The mappings stored in increasing order
    deleted_ranges: std::slice::Iter<'a, DeletedRange>,
}

impl<'a> Iterator for DeletedRanges<'a> {
    type Item = DeletedRangeEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.deleted_ranges.next()?;

        Some(DeletedRangeEntry {
            source: next.source_range.start(),
            transformed: next.transformed_start(),
            text: &self.source_text[next.source_range],
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.deleted_ranges.size_hint()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let last = self.deleted_ranges.last()?;

        Some(DeletedRangeEntry {
            source: last.source_range.start(),
            transformed: last.transformed_start(),
            text: &self.source_text[last.source_range],
        })
    }
}

impl DoubleEndedIterator for DeletedRanges<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let back = self.deleted_ranges.next_back()?;

        Some(DeletedRangeEntry {
            source: back.source_range.start(),
            transformed: back.transformed_start(),
            text: &self.source_text[back.source_range],
        })
    }
}

impl FusedIterator for DeletedRanges<'_> {}
impl ExactSizeIterator for DeletedRanges<'_> {}

#[cfg(test)]
mod tests {
    use crate::source_map::DeletedRangeEntry;
    use crate::{TextRange, TextSize, TransformSourceMapBuilder};
    use ruff_rowan::raw_language::{RawLanguageKind, RawSyntaxTreeBuilder};

    #[test]
    fn range_mapping() {
        let mut cst_builder = RawSyntaxTreeBuilder::new();
        cst_builder.start_node(RawLanguageKind::ROOT);
        // The shape of the tree doesn't matter for the test case
        cst_builder.token(RawLanguageKind::STRING_TOKEN, "(a + (((b + c)) + d)) + e");
        cst_builder.finish_node();
        let root = cst_builder.finish();

        let mut builder = TransformSourceMapBuilder::new();
        builder.push_source_text(&root.text().to_string());

        // Add mappings for all removed parentheses.

        // `(`
        builder.add_deleted_range(TextRange::new(TextSize::from(0), TextSize::from(1)));

        // `(((`
        builder.add_deleted_range(TextRange::new(TextSize::from(5), TextSize::from(6)));
        // Ranges can be added out of order
        builder.add_deleted_range(TextRange::new(TextSize::from(7), TextSize::from(8)));
        builder.add_deleted_range(TextRange::new(TextSize::from(6), TextSize::from(7)));

        // `))`
        builder.add_deleted_range(TextRange::new(TextSize::from(13), TextSize::from(14)));
        builder.add_deleted_range(TextRange::new(TextSize::from(14), TextSize::from(15)));

        // `))`
        builder.add_deleted_range(TextRange::new(TextSize::from(19), TextSize::from(20)));
        builder.add_deleted_range(TextRange::new(TextSize::from(20), TextSize::from(21)));

        let source_map = builder.finish();

        // The following mapping assume the transformed string to be (including whitespace):
        // "a + b + c + d + e";

        // `a`
        assert_eq!(
            source_map.source_range(TextRange::new(TextSize::from(0), TextSize::from(1))),
            TextRange::new(TextSize::from(1), TextSize::from(2))
        );

        // `b`
        assert_eq!(
            source_map.source_range(TextRange::new(TextSize::from(4), TextSize::from(5))),
            TextRange::new(TextSize::from(8), TextSize::from(9))
        );

        // `c`
        assert_eq!(
            source_map.source_range(TextRange::new(TextSize::from(8), TextSize::from(9))),
            TextRange::new(TextSize::from(12), TextSize::from(13))
        );

        // `d`
        assert_eq!(
            source_map.source_range(TextRange::new(TextSize::from(12), TextSize::from(13))),
            TextRange::new(TextSize::from(18), TextSize::from(19))
        );

        // `e`
        assert_eq!(
            source_map.source_range(TextRange::new(TextSize::from(16), TextSize::from(17))),
            TextRange::new(TextSize::from(24), TextSize::from(25))
        );
    }

    #[test]
    fn trimmed_range() {
        // Build up a tree for `((a))`
        // Don't mind the unknown nodes, it doesn't really matter what the nodes are.
        let mut cst_builder = RawSyntaxTreeBuilder::new();
        cst_builder.start_node(RawLanguageKind::ROOT);

        cst_builder.start_node(RawLanguageKind::BOGUS);
        cst_builder.token(RawLanguageKind::STRING_TOKEN, "(");

        cst_builder.start_node(RawLanguageKind::BOGUS);
        cst_builder.token(RawLanguageKind::BOGUS, "(");

        cst_builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
        cst_builder.token(RawLanguageKind::STRING_TOKEN, "a");
        cst_builder.finish_node();

        cst_builder.token(RawLanguageKind::BOGUS, ")");
        cst_builder.finish_node();

        cst_builder.token(RawLanguageKind::BOGUS, ")");
        cst_builder.finish_node();

        cst_builder.token(RawLanguageKind::BOGUS, ";");

        cst_builder.finish_node();

        let root = cst_builder.finish();

        assert_eq!(&root.text(), "((a));");

        let mut bogus = root
            .descendants()
            .filter(|node| node.kind() == RawLanguageKind::BOGUS);

        // `((a))`
        let outer = bogus.next().unwrap();

        // `(a)`
        let inner = bogus.next().unwrap();

        // `a`
        let expression = root
            .descendants()
            .find(|node| node.kind() == RawLanguageKind::LITERAL_EXPRESSION)
            .unwrap();

        let mut builder = TransformSourceMapBuilder::new();
        builder.push_source_text(&root.text().to_string());

        // Add mappings for all removed parentheses.
        builder.add_deleted_range(TextRange::new(TextSize::from(0), TextSize::from(2)));
        builder.add_deleted_range(TextRange::new(TextSize::from(3), TextSize::from(5)));

        // Extend `a` to the range of `(a)`
        builder
            .extend_trimmed_node_range(expression.text_trimmed_range(), inner.text_trimmed_range());
        // Extend `(a)` to the range of `((a))`
        builder.extend_trimmed_node_range(inner.text_trimmed_range(), outer.text_trimmed_range());

        let source_map = builder.finish();

        // Query `a`
        assert_eq!(
            source_map.trimmed_source_text_from_transformed_range(TextRange::new(
                TextSize::from(0),
                TextSize::from(1)
            )),
            "((a))"
        );

        // Query `a;` expression
        assert_eq!(
            source_map.trimmed_source_text_from_transformed_range(TextRange::new(
                TextSize::from(0),
                TextSize::from(2)
            )),
            "((a));"
        );
    }

    #[test]
    fn deleted_ranges() {
        let mut cst_builder = RawSyntaxTreeBuilder::new();
        cst_builder.start_node(RawLanguageKind::ROOT);
        // The shape of the tree doesn't matter for the test case
        cst_builder.token(RawLanguageKind::STRING_TOKEN, "(a + (((b + c)) + d)) + e");
        cst_builder.finish_node();
        let root = cst_builder.finish();

        let mut builder = TransformSourceMapBuilder::new();
        builder.push_source_text(&root.text().to_string());

        // Add mappings for all removed parentheses.

        // `(`
        builder.add_deleted_range(TextRange::new(TextSize::from(0), TextSize::from(1)));

        // `(((`
        builder.add_deleted_range(TextRange::new(TextSize::from(5), TextSize::from(6)));
        // Ranges can be added out of order
        builder.add_deleted_range(TextRange::new(TextSize::from(7), TextSize::from(8)));
        builder.add_deleted_range(TextRange::new(TextSize::from(6), TextSize::from(7)));

        // `))`
        builder.add_deleted_range(TextRange::new(TextSize::from(13), TextSize::from(14)));
        builder.add_deleted_range(TextRange::new(TextSize::from(14), TextSize::from(15)));

        // `))`
        builder.add_deleted_range(TextRange::new(TextSize::from(19), TextSize::from(20)));
        builder.add_deleted_range(TextRange::new(TextSize::from(20), TextSize::from(21)));

        let source_map = builder.finish();

        let deleted_ranges = source_map.deleted_ranges().collect::<Vec<_>>();

        assert_eq!(
            deleted_ranges,
            vec![
                DeletedRangeEntry {
                    source: TextSize::from(0),
                    transformed: TextSize::from(0),
                    text: "("
                },
                DeletedRangeEntry {
                    source: TextSize::from(5),
                    transformed: TextSize::from(4),
                    text: "((("
                },
                DeletedRangeEntry {
                    source: TextSize::from(13),
                    transformed: TextSize::from(9),
                    text: "))"
                },
                DeletedRangeEntry {
                    source: TextSize::from(19),
                    transformed: TextSize::from(13),
                    text: "))"
                },
            ]
        );

        assert_eq!(
            source_map.deleted_ranges().last(),
            Some(DeletedRangeEntry {
                source: TextSize::from(19),
                transformed: TextSize::from(13),
                text: "))"
            })
        );
    }
}
