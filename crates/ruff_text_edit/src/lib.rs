//! Representation of a `TextEdit`.
//!
//! This is taken from [rust-analyzer's `text_edit` crate](https://rust-analyzer.github.io/rust-analyzer/text_edit/index.html)

#![warn(
    rust_2018_idioms,
    unused_lifetimes,
    semicolon_in_expressions_from_macros
)]

use std::{cmp::Ordering, num::NonZeroU32};

use ruff_text_size::{TextRange, TextSize};
use serde::{Deserialize, Serialize};
pub use similar::ChangeTag;
use similar::{utils::TextDiffRemapper, TextDiff};

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TextEdit {
    dictionary: String,
    ops: Vec<CompressedOp>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CompressedOp {
    DiffOp(DiffOp),
    EqualLines { line_count: NonZeroU32 },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DiffOp {
    Equal { range: TextRange },
    Insert { range: TextRange },
    Delete { range: TextRange },
}

impl DiffOp {
    pub fn tag(self) -> ChangeTag {
        match self {
            DiffOp::Equal { .. } => ChangeTag::Equal,
            DiffOp::Insert { .. } => ChangeTag::Insert,
            DiffOp::Delete { .. } => ChangeTag::Delete,
        }
    }

    pub fn text(self, diff: &TextEdit) -> &str {
        let range = match self {
            DiffOp::Equal { range } => range,
            DiffOp::Insert { range } => range,
            DiffOp::Delete { range } => range,
        };

        diff.get_text(range)
    }
}

#[derive(Debug, Default, Clone)]
pub struct TextEditBuilder {
    index: Vec<TextRange>,
    edit: TextEdit,
}

impl TextEdit {
    /// Convenience method for creating a new [`TextEditBuilder`]
    pub fn builder() -> TextEditBuilder {
        TextEditBuilder::default()
    }

    /// Create a diff of `old` to `new`, tokenized by Unicode words
    pub fn from_unicode_words(old: &str, new: &str) -> Self {
        let mut builder = Self::builder();

        let diff = TextDiff::configure()
            .newline_terminated(true)
            .diff_unicode_words(old, new);

        let remapper = TextDiffRemapper::from_text_diff(&diff, old, new);

        for (tag, text) in diff.ops().iter().flat_map(|op| remapper.iter_slices(op)) {
            match tag {
                ChangeTag::Equal => {
                    builder.equal(text);
                }
                ChangeTag::Delete => {
                    builder.delete(text);
                }
                ChangeTag::Insert => {
                    builder.insert(text);
                }
            }
        }

        builder.finish()
    }

    /// Returns the number of [`DiffOp`] in this [`TextEdit`]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Return `true` is this [`TextEdit`] doesn't contain any [`DiffOp`]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns an [Iterator] over the [`DiffOp`] of this [`TextEdit`]
    pub fn iter(&self) -> std::slice::Iter<'_, CompressedOp> {
        self.into_iter()
    }

    /// Return the text value of range interned in this [`TextEdit`] dictionary
    pub fn get_text(&self, range: TextRange) -> &str {
        &self.dictionary[range]
    }

    /// Return the content of the "new" revision of the text represented in
    /// this [`TextEdit`]. This methods needs to be provided with the "old"
    /// revision of the string since [`TextEdit`] doesn't store the content of
    /// text sections that are equal between revisions
    pub fn new_string(&self, old_string: &str) -> String {
        let mut output = String::new();
        let mut input_position = TextSize::from(0);

        for op in &self.ops {
            match op {
                CompressedOp::DiffOp(DiffOp::Equal { range }) => {
                    output.push_str(&self.dictionary[*range]);
                    input_position += range.len();
                }
                CompressedOp::DiffOp(DiffOp::Insert { range }) => {
                    output.push_str(&self.dictionary[*range]);
                }
                CompressedOp::DiffOp(DiffOp::Delete { range }) => {
                    input_position += range.len();
                }
                CompressedOp::EqualLines { line_count } => {
                    let start = u32::from(input_position) as usize;
                    let input = &old_string[start..];

                    let line_break_count = line_count.get() as usize + 1;
                    for line in input.split_inclusive('\n').take(line_break_count) {
                        output.push_str(line);
                        input_position += TextSize::of(line);
                    }
                }
            }
        }

        output
    }
}

impl IntoIterator for TextEdit {
    type Item = CompressedOp;
    type IntoIter = std::vec::IntoIter<CompressedOp>;

    fn into_iter(self) -> Self::IntoIter {
        self.ops.into_iter()
    }
}

impl<'a> IntoIterator for &'a TextEdit {
    type Item = &'a CompressedOp;
    type IntoIter = std::slice::Iter<'a, CompressedOp>;

    fn into_iter(self) -> Self::IntoIter {
        self.ops.iter()
    }
}

impl TextEditBuilder {
    pub fn is_empty(&self) -> bool {
        self.edit.ops.is_empty()
    }

    /// Add a piece of string to the dictionary, returning the corresponding
    /// range in the dictionary string
    fn intern(&mut self, value: &str) -> TextRange {
        let value_bytes = value.as_bytes();
        let value_len = TextSize::of(value);

        let index = self.index.binary_search_by(|range| {
            let entry = self.edit.dictionary[*range].as_bytes();

            for (lhs, rhs) in entry.iter().zip(value_bytes) {
                match lhs.cmp(rhs) {
                    Ordering::Equal => continue,
                    ordering => return ordering,
                }
            }

            match entry.len().cmp(&value_bytes.len()) {
                // If all bytes in the shared sub-slice match, the dictionary
                // entry is allowed to be longer than the text being inserted
                Ordering::Greater => Ordering::Equal,
                ordering => ordering,
            }
        });

        match index {
            Ok(index) => {
                let range = self.index[index];
                let len = value_len.min(range.len());
                TextRange::at(range.start(), len)
            }
            Err(index) => {
                let start = TextSize::of(&self.edit.dictionary);
                self.edit.dictionary.push_str(value);

                let range = TextRange::at(start, value_len);
                self.index.insert(index, range);
                range
            }
        }
    }

    pub fn equal(&mut self, text: &str) {
        if let Some((start, mid, end)) = compress_equal_op(text) {
            let start = self.intern(start);
            self.edit
                .ops
                .push(CompressedOp::DiffOp(DiffOp::Equal { range: start }));

            self.edit
                .ops
                .push(CompressedOp::EqualLines { line_count: mid });

            let end = self.intern(end);
            self.edit
                .ops
                .push(CompressedOp::DiffOp(DiffOp::Equal { range: end }));
        } else {
            let range = self.intern(text);
            self.edit
                .ops
                .push(CompressedOp::DiffOp(DiffOp::Equal { range }));
        }
    }

    pub fn insert(&mut self, text: &str) {
        let range = self.intern(text);
        self.edit
            .ops
            .push(CompressedOp::DiffOp(DiffOp::Insert { range }));
    }

    pub fn delete(&mut self, text: &str) {
        let range = self.intern(text);
        self.edit
            .ops
            .push(CompressedOp::DiffOp(DiffOp::Delete { range }));
    }

    pub fn replace(&mut self, old: &str, new: &str) {
        self.delete(old);
        self.insert(new);
    }

    pub fn finish(self) -> TextEdit {
        self.edit
    }
}

/// Number of lines to keep as [`DiffOp::Equal`] operations around a
/// [`CompressedOp::EqualCompressedLines`] operation. This has the effect of
/// making the compressed diff retain a few line of equal content around
/// changes, which is useful for display as it makes it possible to print a few
/// context lines around changes without having to keep the full original text
/// around.
const COMPRESSED_DIFFS_CONTEXT_LINES: usize = 2;

fn compress_equal_op(text: &str) -> Option<(&str, NonZeroU32, &str)> {
    let mut iter = text.split('\n');

    let mut leading_len = COMPRESSED_DIFFS_CONTEXT_LINES;
    for _ in 0..=COMPRESSED_DIFFS_CONTEXT_LINES {
        leading_len += iter.next()?.len();
    }

    let mut trailing_len = COMPRESSED_DIFFS_CONTEXT_LINES;
    for _ in 0..=COMPRESSED_DIFFS_CONTEXT_LINES {
        trailing_len += iter.next_back()?.len();
    }

    let mid_count = iter.count();
    let mid_count = u32::try_from(mid_count).ok()?;
    let mid_count = NonZeroU32::new(mid_count)?;

    let trailing_start = text.len().saturating_sub(trailing_len);

    Some((&text[..leading_len], mid_count, &text[trailing_start..]))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use crate::{compress_equal_op, TextEdit};

    #[test]
    fn compress_short() {
        let output = compress_equal_op(
            "
start 1
start 2
end 1
end 2
",
        );

        assert_eq!(output, None);
    }

    #[test]
    fn compress_long() {
        let output = compress_equal_op(
            "
start 1
start 2
mid 1
mid 2
mid 3
end 1
end 2
",
        );

        assert_eq!(
            output,
            Some((
                "\nstart 1\nstart 2",
                NonZeroU32::new(3).unwrap(),
                "end 1\nend 2\n"
            ))
        );
    }

    #[test]
    fn new_string_compressed() {
        const OLD: &str = "line 1 old
line 2
line 3
line 4
line 5
line 6
line 7 old";

        const NEW: &str = "line 1 new
line 2
line 3
line 4
line 5
line 6
line 7 new";

        let diff = TextEdit::from_unicode_words(OLD, NEW);
        let new_string = diff.new_string(OLD);

        assert_eq!(new_string, NEW);
    }
}
