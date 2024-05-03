pub mod document;
pub mod tag;

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::iter::FusedIterator;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::rc::Rc;
use unicode_width::UnicodeWidthChar;

use crate::format_element::tag::{GroupMode, LabelId, Tag};
use crate::source_code::SourceCodeSlice;
use crate::{IndentWidth, TagKind};
use ruff_text_size::TextSize;

/// Language agnostic IR for formatting source code.
///
/// Use the helper functions like [`crate::builders::space`], [`crate::builders::soft_line_break`] etc. defined in this file to create elements.
#[derive(Clone, Eq, PartialEq)]
pub enum FormatElement {
    /// A space token, see [`crate::builders::space`] for documentation.
    Space,

    /// A new line, see [`crate::builders::soft_line_break`], [`crate::builders::hard_line_break`], and [`crate::builders::soft_line_break_or_space`] for documentation.
    Line(LineMode),

    /// Forces the parent group to print in expanded mode.
    ExpandParent,

    /// Indicates the position of the elements coming after this element in the source document.
    /// The printer will create a source map entry from this position in the source document to the
    /// formatted position.
    SourcePosition(TextSize),

    /// A ASCII only Token that contains no line breaks or tab characters.
    Token { text: &'static str },

    /// An arbitrary text that can contain tabs, newlines, and unicode characters.
    Text {
        /// There's no need for the text to be mutable, using `Box<str>` safes 8 bytes over `String`.
        text: Box<str>,
        text_width: TextWidth,
    },

    /// Text that gets emitted as it is in the source code. Optimized to avoid any allocations.
    SourceCodeSlice {
        slice: SourceCodeSlice,
        text_width: TextWidth,
    },

    /// Prevents that line suffixes move past this boundary. Forces the printer to print any pending
    /// line suffixes, potentially by inserting a hard line break.
    LineSuffixBoundary,

    /// An interned format element. Useful when the same content must be emitted multiple times to avoid
    /// deep cloning the IR when using the `best_fitting!` macro or `if_group_fits_on_line` and `if_group_breaks`.
    Interned(Interned),

    /// A list of different variants representing the same content. The printer picks the best fitting content.
    /// Line breaks inside of a best fitting don't propagate to parent groups.
    BestFitting {
        variants: BestFittingVariants,
        mode: BestFittingMode,
    },

    /// A [Tag] that marks the start/end of some content to which some special formatting is applied.
    Tag(Tag),
}

impl FormatElement {
    pub fn tag_kind(&self) -> Option<TagKind> {
        if let FormatElement::Tag(tag) = self {
            Some(tag.kind())
        } else {
            None
        }
    }
}

impl std::fmt::Debug for FormatElement {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FormatElement::Space => write!(fmt, "Space"),
            FormatElement::Line(mode) => fmt.debug_tuple("Line").field(mode).finish(),
            FormatElement::ExpandParent => write!(fmt, "ExpandParent"),
            FormatElement::Token { text } => fmt.debug_tuple("Token").field(text).finish(),
            FormatElement::Text { text, .. } => fmt.debug_tuple("DynamicText").field(text).finish(),
            FormatElement::SourceCodeSlice { slice, text_width } => fmt
                .debug_tuple("Text")
                .field(slice)
                .field(text_width)
                .finish(),
            FormatElement::LineSuffixBoundary => write!(fmt, "LineSuffixBoundary"),
            FormatElement::BestFitting { variants, mode } => fmt
                .debug_struct("BestFitting")
                .field("variants", variants)
                .field("mode", &mode)
                .finish(),
            FormatElement::Interned(interned) => fmt.debug_list().entries(&**interned).finish(),
            FormatElement::Tag(tag) => fmt.debug_tuple("Tag").field(tag).finish(),
            FormatElement::SourcePosition(position) => {
                fmt.debug_tuple("SourcePosition").field(position).finish()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LineMode {
    /// See [`crate::builders::soft_line_break_or_space`] for documentation.
    SoftOrSpace,
    /// See [`crate::builders::soft_line_break`] for documentation.
    Soft,
    /// See [`crate::builders::hard_line_break`] for documentation.
    Hard,
    /// See [`crate::builders::empty_line`] for documentation.
    Empty,
}

impl LineMode {
    pub const fn is_hard(&self) -> bool {
        matches!(self, LineMode::Hard)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PrintMode {
    /// Omits any soft line breaks
    Flat,
    /// Prints soft line breaks as line breaks
    Expanded,
}

impl PrintMode {
    pub const fn is_flat(&self) -> bool {
        matches!(self, PrintMode::Flat)
    }

    pub const fn is_expanded(&self) -> bool {
        matches!(self, PrintMode::Expanded)
    }
}

impl From<GroupMode> for PrintMode {
    fn from(value: GroupMode) -> Self {
        match value {
            GroupMode::Flat => PrintMode::Flat,
            GroupMode::Expand | GroupMode::Propagated => PrintMode::Expanded,
        }
    }
}

#[derive(Clone)]
pub struct Interned(Rc<[FormatElement]>);

impl Interned {
    pub(super) fn new(content: Vec<FormatElement>) -> Self {
        Self(content.into())
    }
}

impl PartialEq for Interned {
    fn eq(&self, other: &Interned) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Interned {}

impl Hash for Interned {
    fn hash<H>(&self, hasher: &mut H)
    where
        H: Hasher,
    {
        Rc::as_ptr(&self.0).hash(hasher);
    }
}

impl std::fmt::Debug for Interned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Interned {
    type Target = [FormatElement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

const LINE_SEPARATOR: char = '\u{2028}';
const PARAGRAPH_SEPARATOR: char = '\u{2029}';
pub const LINE_TERMINATORS: [char; 3] = ['\r', LINE_SEPARATOR, PARAGRAPH_SEPARATOR];

/// Replace the line terminators matching the provided list with "\n"
/// since its the only line break type supported by the printer
pub fn normalize_newlines<const N: usize>(text: &str, terminators: [char; N]) -> Cow<str> {
    let mut result = String::new();
    let mut last_end = 0;

    for (start, part) in text.match_indices(terminators) {
        result.push_str(&text[last_end..start]);
        result.push('\n');

        last_end = start + part.len();
        // If the current character is \r and the
        // next is \n, skip over the entire sequence
        if part == "\r" && text[last_end..].starts_with('\n') {
            last_end += 1;
        }
    }

    // If the result is empty no line terminators were matched,
    // return the entire input text without allocating a new String
    if result.is_empty() {
        Cow::Borrowed(text)
    } else {
        result.push_str(&text[last_end..text.len()]);
        Cow::Owned(result)
    }
}

impl FormatElement {
    /// Returns `true` if self is a [`FormatElement::Tag`]
    pub const fn is_tag(&self) -> bool {
        matches!(self, FormatElement::Tag(_))
    }

    /// Returns `true` if self is a [`FormatElement::Tag`] and [`Tag::is_start`] is `true`.
    pub const fn is_start_tag(&self) -> bool {
        match self {
            FormatElement::Tag(tag) => tag.is_start(),
            _ => false,
        }
    }

    /// Returns `true` if self is a [`FormatElement::Tag`] and [`Tag::is_end`] is `true`.
    pub const fn is_end_tag(&self) -> bool {
        match self {
            FormatElement::Tag(tag) => tag.is_end(),
            _ => false,
        }
    }

    pub const fn is_text(&self) -> bool {
        matches!(
            self,
            FormatElement::SourceCodeSlice { .. }
                | FormatElement::Text { .. }
                | FormatElement::Token { .. }
        )
    }

    pub const fn is_space(&self) -> bool {
        matches!(self, FormatElement::Space)
    }
}

impl FormatElements for FormatElement {
    fn will_break(&self) -> bool {
        match self {
            FormatElement::ExpandParent => true,
            FormatElement::Tag(Tag::StartGroup(group)) => !group.mode().is_flat(),
            FormatElement::Line(line_mode) => matches!(line_mode, LineMode::Hard | LineMode::Empty),
            FormatElement::Text { text_width, .. } => text_width.is_multiline(),
            FormatElement::SourceCodeSlice { text_width, .. } => text_width.is_multiline(),
            FormatElement::Interned(interned) => interned.will_break(),
            // Traverse into the most flat version because the content is guaranteed to expand when even
            // the most flat version contains some content that forces a break.
            FormatElement::BestFitting {
                variants: best_fitting,
                ..
            } => best_fitting.most_flat().will_break(),
            FormatElement::LineSuffixBoundary
            | FormatElement::Space
            | FormatElement::Tag(_)
            | FormatElement::Token { .. }
            | FormatElement::SourcePosition(_) => false,
        }
    }

    fn has_label(&self, label_id: LabelId) -> bool {
        match self {
            FormatElement::Tag(Tag::StartLabelled(actual)) => *actual == label_id,
            FormatElement::Interned(interned) => interned.deref().has_label(label_id),
            _ => false,
        }
    }

    fn start_tag(&self, _: TagKind) -> Option<&Tag> {
        None
    }

    fn end_tag(&self, kind: TagKind) -> Option<&Tag> {
        match self {
            FormatElement::Tag(tag) if tag.kind() == kind && tag.is_end() => Some(tag),
            _ => None,
        }
    }
}

/// Mode used to determine if any variant (except the most expanded) fits for [`BestFittingVariants`].
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum BestFittingMode {
    /// The variant fits if the content up to the first hard or a soft line break inside a [`Group`] with
    /// [`PrintMode::Expanded`] fits on the line. The default mode.
    ///
    /// [`Group`]: tag::Group
    #[default]
    FirstLine,

    /// A variant fits if all lines fit into the configured print width. A line ends if by any
    /// hard or a soft line break inside a [`Group`] with [`PrintMode::Expanded`].
    /// The content doesn't fit if there's any hard line break  outside a [`Group`] with [`PrintMode::Expanded`]
    /// (a hard line break in content that should be considered in [`PrintMode::Flat`].
    ///
    /// Use this mode with caution as it requires measuring all content of the variant which is more
    /// expensive than using [`BestFittingMode::FirstLine`].
    ///
    /// [`Group`]: tag::Group
    AllLines,
}

/// The different variants for this element.
/// The first element is the one that takes up the most space horizontally (the most flat),
/// The last element takes up the least space horizontally (but most horizontal space).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct BestFittingVariants(Box<[FormatElement]>);

impl BestFittingVariants {
    /// Creates a new best fitting IR with the given variants.
    ///
    /// Callers are required to ensure that the number of variants given
    /// is at least 2 when using `most_expanded` or `most_flag`.
    ///
    /// You're looking for a way to create a `BestFitting` object, use the `best_fitting![least_expanded, most_expanded]` macro.
    #[doc(hidden)]
    pub fn from_vec_unchecked(variants: Vec<FormatElement>) -> Self {
        debug_assert!(
            variants
                .iter()
                .filter(|element| matches!(element, FormatElement::Tag(Tag::StartBestFittingEntry)))
                .count()
                >= 2,
            "Requires at least the least expanded and most expanded variants"
        );
        Self(variants.into_boxed_slice())
    }

    /// Returns the most expanded variant
    ///
    /// # Panics
    ///
    /// When the number of variants is less than two.
    pub fn most_expanded(&self) -> &[FormatElement] {
        assert!(
            self.as_slice()
                .iter()
                .filter(|element| matches!(element, FormatElement::Tag(Tag::StartBestFittingEntry)))
                .count()
                >= 2,
            "Requires at least the least expanded and most expanded variants"
        );
        self.into_iter().last().unwrap()
    }

    pub fn as_slice(&self) -> &[FormatElement] {
        &self.0
    }

    /// Returns the least expanded variant
    ///
    /// # Panics
    ///
    /// When the number of variants is less than two.
    pub fn most_flat(&self) -> &[FormatElement] {
        assert!(
            self.as_slice()
                .iter()
                .filter(|element| matches!(element, FormatElement::Tag(Tag::StartBestFittingEntry)))
                .count()
                >= 2,
            "Requires at least the least expanded and most expanded variants"
        );
        self.into_iter().next().unwrap()
    }
}

impl Deref for BestFittingVariants {
    type Target = [FormatElement];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

pub struct BestFittingVariantsIter<'a> {
    elements: &'a [FormatElement],
}

impl<'a> IntoIterator for &'a BestFittingVariants {
    type Item = &'a [FormatElement];
    type IntoIter = BestFittingVariantsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        BestFittingVariantsIter { elements: &self.0 }
    }
}

impl<'a> Iterator for BestFittingVariantsIter<'a> {
    type Item = &'a [FormatElement];

    fn next(&mut self) -> Option<Self::Item> {
        match self.elements.first()? {
            FormatElement::Tag(Tag::StartBestFittingEntry) => {
                let end = self
                    .elements
                    .iter()
                    .position(|element| {
                        matches!(element, FormatElement::Tag(Tag::EndBestFittingEntry))
                    })
                    .map_or(self.elements.len(), |position| position + 1);

                let (variant, rest) = self.elements.split_at(end);
                self.elements = rest;

                Some(variant)
            }
            _ => None,
        }
    }

    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.next_back()
    }
}

impl<'a> DoubleEndedIterator for BestFittingVariantsIter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let start_position = self.elements.iter().rposition(|element| {
            matches!(element, FormatElement::Tag(Tag::StartBestFittingEntry))
        })?;

        let (rest, variant) = self.elements.split_at(start_position);
        self.elements = rest;
        Some(variant)
    }
}

impl FusedIterator for BestFittingVariantsIter<'_> {}

pub trait FormatElements {
    /// Returns true if this [`FormatElement`] is guaranteed to break across multiple lines by the printer.
    /// This is the case if this format element recursively contains a:
    /// - [`crate::builders::empty_line`] or [`crate::builders::hard_line_break`]
    /// - A token containing '\n'
    ///
    /// Use this with caution, this is only a heuristic and the printer may print the element over multiple
    /// lines if this element is part of a group and the group doesn't fit on a single line.
    fn will_break(&self) -> bool;

    /// Returns true if the element has the given label.
    fn has_label(&self, label: LabelId) -> bool;

    /// Returns the start tag of `kind` if:
    /// - the last element is an end tag of `kind`.
    /// - there's a matching start tag in this document (may not be true if this slice is an interned element and the `start` is in the document storing the interned element).
    fn start_tag(&self, kind: TagKind) -> Option<&Tag>;

    /// Returns the end tag if:
    /// - the last element is an end tag of `kind`
    fn end_tag(&self, kind: TagKind) -> Option<&Tag>;
}

/// New-type wrapper for a single-line text unicode width.
/// Mainly to prevent access to the inner value.
///
/// ## Representation
///
/// Represents the width by adding 1 to the actual width so that the width can be represented by a [`NonZeroU32`],
/// allowing [`TextWidth`] or [`Option<Width>`] fit in 4 bytes rather than 8.
///
/// This means that 2^32 can not be precisely represented and instead has the same value as 2^32-1.
/// This imprecision shouldn't matter in practice because either text are longer than any configured line width
/// and thus, the text should break.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Width(NonZeroU32);

impl Width {
    pub(crate) const fn new(width: u32) -> Self {
        Width(NonZeroU32::MIN.saturating_add(width))
    }

    pub const fn value(self) -> u32 {
        self.0.get() - 1
    }
}

/// The pre-computed unicode width of a text if it is a single-line text or a marker
/// that it is a multiline text if it contains a line feed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TextWidth {
    Width(Width),
    Multiline,
}

impl TextWidth {
    pub fn from_text(text: &str, indent_width: IndentWidth) -> TextWidth {
        let mut width = 0u32;

        for c in text.chars() {
            let char_width = match c {
                '\t' => indent_width.value(),
                '\n' => return TextWidth::Multiline,
                #[allow(clippy::cast_possible_truncation)]
                c => c.width().unwrap_or(0) as u32,
            };
            width += char_width;
        }

        Self::Width(Width::new(width))
    }

    pub const fn width(self) -> Option<Width> {
        match self {
            TextWidth::Width(width) => Some(width),
            TextWidth::Multiline => None,
        }
    }

    pub(crate) const fn is_multiline(self) -> bool {
        matches!(self, TextWidth::Multiline)
    }
}

#[cfg(test)]
mod tests {

    use crate::format_element::{normalize_newlines, LINE_TERMINATORS};

    #[test]
    fn test_normalize_newlines() {
        assert_eq!(normalize_newlines("a\nb", LINE_TERMINATORS), "a\nb");
        assert_eq!(normalize_newlines("a\n\n\nb", LINE_TERMINATORS), "a\n\n\nb");
        assert_eq!(normalize_newlines("a\rb", LINE_TERMINATORS), "a\nb");
        assert_eq!(normalize_newlines("a\r\nb", LINE_TERMINATORS), "a\nb");
        assert_eq!(
            normalize_newlines("a\r\n\r\n\r\nb", LINE_TERMINATORS),
            "a\n\n\nb"
        );
        assert_eq!(normalize_newlines("a\u{2028}b", LINE_TERMINATORS), "a\nb");
        assert_eq!(normalize_newlines("a\u{2029}b", LINE_TERMINATORS), "a\nb");
    }
}

#[cfg(target_pointer_width = "64")]
mod sizes {
    // Increasing the size of FormatElement has serious consequences on runtime performance and memory footprint.
    // Is there a more efficient way to encode the data to avoid increasing its size? Can the information
    // be recomputed at a later point in time?
    // You reduced the size of a format element? Excellent work!

    use super::{BestFittingVariants, Interned, TextWidth};
    use static_assertions::assert_eq_size;

    assert_eq_size!(ruff_text_size::TextRange, [u8; 8]);
    assert_eq_size!(TextWidth, [u8; 4]);
    assert_eq_size!(super::tag::VerbatimKind, [u8; 8]);
    assert_eq_size!(Interned, [u8; 16]);
    assert_eq_size!(BestFittingVariants, [u8; 16]);

    #[cfg(not(debug_assertions))]
    assert_eq_size!(crate::SourceCodeSlice, [u8; 8]);

    #[cfg(not(debug_assertions))]
    assert_eq_size!(super::Tag, [u8; 16]);

    #[cfg(not(debug_assertions))]
    assert_eq_size!(super::FormatElement, [u8; 24]);
}
