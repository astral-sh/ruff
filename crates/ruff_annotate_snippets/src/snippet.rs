//! Structures used as an input for the library.
//!
//! Example:
//!
//! ```
//! use ruff_annotate_snippets::*;
//!
//! Level::Error.title("mismatched types")
//!     .snippet(Snippet::source("Foo").line_start(51).origin("src/format.rs"))
//!     .snippet(Snippet::source("Faa").line_start(129).origin("src/display.rs"));
//! ```

use std::ops::Range;

/// Primary structure provided for formatting
///
/// See [`Level::title`] to create a [`Message`]
#[derive(Debug)]
pub struct Message<'a> {
    pub(crate) level: Level,
    pub(crate) id: Option<&'a str>,
    pub(crate) title: &'a str,
    pub(crate) snippets: Vec<Snippet<'a>>,
    pub(crate) footer: Vec<Message<'a>>,
    pub(crate) is_fixable: bool,
}

impl<'a> Message<'a> {
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    pub fn snippet(mut self, slice: Snippet<'a>) -> Self {
        self.snippets.push(slice);
        self
    }

    pub fn snippets(mut self, slice: impl IntoIterator<Item = Snippet<'a>>) -> Self {
        self.snippets.extend(slice);
        self
    }

    pub fn footer(mut self, footer: Message<'a>) -> Self {
        self.footer.push(footer);
        self
    }

    pub fn footers(mut self, footer: impl IntoIterator<Item = Message<'a>>) -> Self {
        self.footer.extend(footer);
        self
    }

    /// Whether or not the diagnostic for this message is fixable.
    ///
    /// This is rendered as a `[*]` indicator after the `id` in an annotation header, if the
    /// annotation also has `Level::None`.
    pub fn is_fixable(mut self, yes: bool) -> Self {
        self.is_fixable = yes;
        self
    }
}

/// Structure containing the slice of text to be annotated and
/// basic information about the location of the slice.
///
/// One `Snippet` is meant to represent a single, continuous,
/// slice of source code that you want to annotate.
#[derive(Debug)]
pub struct Snippet<'a> {
    pub(crate) origin: Option<&'a str>,
    pub(crate) line_start: usize,

    pub(crate) source: &'a str,
    pub(crate) annotations: Vec<Annotation<'a>>,

    pub(crate) fold: bool,

    /// The optional cell index in a Jupyter notebook, used for reporting source locations along
    /// with the ranges on `annotations`.
    pub(crate) cell_index: Option<usize>,
}

impl<'a> Snippet<'a> {
    pub fn source(source: &'a str) -> Self {
        Self {
            origin: None,
            line_start: 1,
            source,
            annotations: vec![],
            fold: false,
            cell_index: None,
        }
    }

    pub fn line_start(mut self, line_start: usize) -> Self {
        self.line_start = line_start;
        self
    }

    pub fn origin(mut self, origin: &'a str) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn annotation(mut self, annotation: Annotation<'a>) -> Self {
        self.annotations.push(annotation);
        self
    }

    pub fn annotations(mut self, annotation: impl IntoIterator<Item = Annotation<'a>>) -> Self {
        self.annotations.extend(annotation);
        self
    }

    /// Hide lines without [`Annotation`]s
    pub fn fold(mut self, fold: bool) -> Self {
        self.fold = fold;
        self
    }

    /// Attach a Jupyter notebook cell index.
    pub fn cell_index(mut self, index: Option<usize>) -> Self {
        self.cell_index = index;
        self
    }
}

/// An annotation for a [`Snippet`].
///
/// See [`Level::span`] to create a [`Annotation`]
#[derive(Debug)]
pub struct Annotation<'a> {
    /// The byte range of the annotation in the `source` string
    pub(crate) range: Range<usize>,
    pub(crate) label: Option<&'a str>,
    pub(crate) level: Level,
    pub(crate) is_file_level: bool,
}

impl<'a> Annotation<'a> {
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn is_file_level(mut self, yes: bool) -> Self {
        self.is_file_level = yes;
        self
    }
}

/// Types of annotations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    /// Do not attach any annotation.
    None,
    /// Error annotations are displayed using red color and "^" character.
    Error,
    /// Warning annotations are displayed using blue color and "-" character.
    Warning,
    Info,
    Note,
    Help,
}

impl Level {
    pub fn title(self, title: &str) -> Message<'_> {
        Message {
            level: self,
            id: None,
            title,
            snippets: vec![],
            footer: vec![],
            is_fixable: false,
        }
    }

    /// Create a [`Annotation`] with the given span for a [`Snippet`]
    pub fn span<'a>(self, span: Range<usize>) -> Annotation<'a> {
        Annotation {
            range: span,
            label: None,
            level: self,
            is_file_level: false,
        }
    }
}
