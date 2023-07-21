pub mod document;
pub mod tag;

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

use crate::format_element::tag::{GroupMode, LabelId, Tag};
use crate::source_code::SourceCodeSlice;
use crate::TagKind;
use ruff_text_size::TextSize;

/// Language agnostic IR for formatting source code.
///
/// Use the helper functions like [crate::builders::space], [crate::builders::soft_line_break] etc. defined in this file to create elements.
#[derive(Clone, Eq, PartialEq)]
pub enum FormatElement {
    /// A space token, see [crate::builders::space] for documentation.
    Space,

    /// A new line, see [crate::builders::soft_line_break], [crate::builders::hard_line_break], and [crate::builders::soft_line_break_or_space] for documentation.
    Line(LineMode),

    /// Forces the parent group to print in expanded mode.
    ExpandParent,

    /// Indicates the position of the elements coming after this element in the source document.
    /// The printer will create a source map entry from this position in the source document to the
    /// formatted position.
    SourcePosition(TextSize),

    /// Token constructed by the formatter from a static string
    StaticText { text: &'static str },

    /// Token constructed from the input source as a dynamic
    /// string.
    DynamicText {
        /// There's no need for the text to be mutable, using `Box<str>` safes 8 bytes over `String`.
        text: Box<str>,
    },

    /// Text that gets emitted as it is in the source code. Optimized to avoid any allocations.
    SourceCodeSlice {
        slice: SourceCodeSlice,
        /// Whether the string contains any new line characters
        contains_newlines: bool,
    },

    /// Prevents that line suffixes move past this boundary. Forces the printer to print any pending
    /// line suffixes, potentially by inserting a hard line break.
    LineSuffixBoundary,

    /// An interned format element. Useful when the same content must be emitted multiple times to avoid
    /// deep cloning the IR when using the `best_fitting!` macro or `if_group_fits_on_line` and `if_group_breaks`.
    Interned(Interned),

    /// A list of different variants representing the same content. The printer picks the best fitting content.
    /// Line breaks inside of a best fitting don't propagate to parent groups.
    BestFitting { variants: BestFittingVariants },

    /// A [Tag] that marks the start/end of some content to which some special formatting is applied.
    Tag(Tag),
}

impl std::fmt::Debug for FormatElement {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FormatElement::Space => write!(fmt, "Space"),
            FormatElement::Line(mode) => fmt.debug_tuple("Line").field(mode).finish(),
            FormatElement::ExpandParent => write!(fmt, "ExpandParent"),
            FormatElement::StaticText { text } => {
                fmt.debug_tuple("StaticText").field(text).finish()
            }
            FormatElement::DynamicText { text, .. } => {
                fmt.debug_tuple("DynamicText").field(text).finish()
            }
            FormatElement::SourceCodeSlice {
                slice,
                contains_newlines,
            } => fmt
                .debug_tuple("Text")
                .field(slice)
                .field(contains_newlines)
                .finish(),
            FormatElement::LineSuffixBoundary => write!(fmt, "LineSuffixBoundary"),
            FormatElement::BestFitting { variants } => fmt
                .debug_struct("BestFitting")
                .field("variants", variants)
                .finish(),
            FormatElement::Interned(interned) => {
                fmt.debug_list().entries(interned.deref()).finish()
            }
            FormatElement::Tag(tag) => fmt.debug_tuple("Tag").field(tag).finish(),
            FormatElement::SourcePosition(position) => {
                fmt.debug_tuple("SourcePosition").field(position).finish()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LineMode {
    /// See [crate::builders::soft_line_break_or_space] for documentation.
    SoftOrSpace,
    /// See [crate::builders::soft_line_break] for documentation.
    Soft,
    /// See [crate::builders::hard_line_break] for documentation.
    Hard,
    /// See [crate::builders::empty_line] for documentation.
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
        self.0.deref()
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
    /// Returns `true` if self is a [FormatElement::Tag]
    pub const fn is_tag(&self) -> bool {
        matches!(self, FormatElement::Tag(_))
    }

    /// Returns `true` if self is a [FormatElement::Tag] and [Tag::is_start] is `true`.
    pub const fn is_start_tag(&self) -> bool {
        match self {
            FormatElement::Tag(tag) => tag.is_start(),
            _ => false,
        }
    }

    /// Returns `true` if self is a [FormatElement::Tag] and [Tag::is_end] is `true`.
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
                | FormatElement::DynamicText { .. }
                | FormatElement::StaticText { .. }
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
            FormatElement::StaticText { text } => text.contains('\n'),
            FormatElement::DynamicText { text, .. } => text.contains('\n'),
            FormatElement::SourceCodeSlice {
                contains_newlines, ..
            } => *contains_newlines,
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

/// The different variants for this element.
/// The first element is the one that takes up the most space horizontally (the most flat),
/// The last element takes up the least space horizontally (but most horizontal space).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct BestFittingVariants(Box<[Box<[FormatElement]>]>);

impl BestFittingVariants {
    /// Creates a new best fitting IR with the given variants. The method itself isn't unsafe
    /// but it is to discourage people from using it because the printer will panic if
    /// the slice doesn't contain at least the least and most expanded variants.
    ///
    /// You're looking for a way to create a `BestFitting` object, use the `best_fitting![least_expanded, most_expanded]` macro.
    ///
    /// ## Safety
    /// The slice must contain at least two variants.
    #[doc(hidden)]
    pub unsafe fn from_vec_unchecked(variants: Vec<Box<[FormatElement]>>) -> Self {
        debug_assert!(
            variants.len() >= 2,
            "Requires at least the least expanded and most expanded variants"
        );

        Self(variants.into_boxed_slice())
    }

    /// Returns the most expanded variant
    pub fn most_expanded(&self) -> &[FormatElement] {
        self.0.last().expect(
            "Most contain at least two elements, as guaranteed by the best fitting builder.",
        )
    }

    pub fn as_slice(&self) -> &[Box<[FormatElement]>] {
        &self.0
    }

    /// Returns the least expanded variant
    pub fn most_flat(&self) -> &[FormatElement] {
        self.0.first().expect(
            "Most contain at least two elements, as guaranteed by the best fitting builder.",
        )
    }
}

impl Deref for BestFittingVariants {
    type Target = [Box<[FormatElement]>];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a> IntoIterator for &'a BestFittingVariants {
    type Item = &'a Box<[FormatElement]>;
    type IntoIter = std::slice::Iter<'a, Box<[FormatElement]>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

pub trait FormatElements {
    /// Returns true if this [FormatElement] is guaranteed to break across multiple lines by the printer.
    /// This is the case if this format element recursively contains a:
    /// - [crate::builders::empty_line] or [crate::builders::hard_line_break]
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

    use static_assertions::assert_eq_size;

    assert_eq_size!(ruff_text_size::TextRange, [u8; 8]);
    assert_eq_size!(crate::prelude::tag::VerbatimKind, [u8; 8]);
    assert_eq_size!(crate::prelude::Interned, [u8; 16]);
    assert_eq_size!(crate::format_element::BestFittingVariants, [u8; 16]);

    #[cfg(not(debug_assertions))]
    assert_eq_size!(crate::SourceCodeSlice, [u8; 8]);

    #[cfg(not(debug_assertions))]
    assert_eq_size!(crate::format_element::Tag, [u8; 16]);

    #[cfg(not(debug_assertions))]
    assert_eq_size!(crate::FormatElement, [u8; 24]);
}
