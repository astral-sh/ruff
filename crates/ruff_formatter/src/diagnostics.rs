use crate::prelude::TagKind;
use crate::GroupId;
use ruff_text_size::TextRange;
use std::error::Error;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Series of errors encountered during formatting
pub enum FormatError {
    /// In case a node can't be formatted because it either misses a require child element or
    /// a child is present that should not (e.g. a trailing comma after a rest element).
    SyntaxError { message: &'static str },
    /// In case range formatting failed because the provided range was larger
    /// than the formatted syntax tree
    RangeError { input: TextRange, tree: TextRange },

    /// In case printing the document failed because it has an invalid structure.
    InvalidDocument(InvalidDocumentError),

    /// Formatting failed because some content encountered a situation where a layout
    /// choice by an enclosing [`crate::Format`] resulted in a poor layout for a child [`crate::Format`].
    ///
    /// It's up to an enclosing [`crate::Format`] to handle the error and pick another layout.
    /// This error should not be raised if there's no outer [`crate::Format`] handling the poor layout error,
    /// avoiding that formatting of the whole document fails.
    PoorLayout,
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::SyntaxError {message} => {
                std::write!(fmt, "syntax error: {message}")
            },
            FormatError::RangeError { input, tree } => std::write!(
                fmt,
                "formatting range {input:?} is larger than syntax tree {tree:?}"
            ),
            FormatError::InvalidDocument(error) => std::write!(fmt, "Invalid document: {error}\n\n This is an internal Rome error. Please report if necessary."),
            FormatError::PoorLayout => {
                std::write!(fmt, "Poor layout: The formatter wasn't able to pick a good layout for your document. This is an internal Rome error. Please report if necessary.")
            }
        }
    }
}

impl Error for FormatError {}

impl From<PrintError> for FormatError {
    fn from(error: PrintError) -> Self {
        FormatError::from(&error)
    }
}

impl From<&PrintError> for FormatError {
    fn from(error: &PrintError) -> Self {
        match error {
            PrintError::InvalidDocument(reason) => FormatError::InvalidDocument(*reason),
        }
    }
}

impl FormatError {
    pub fn syntax_error(message: &'static str) -> Self {
        Self::SyntaxError { message }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InvalidDocumentError {
    /// Mismatching start/end kinds
    ///
    /// ```plain
    /// StartIndent
    /// ...
    /// EndGroup
    /// ```
    StartEndTagMismatch {
        start_kind: TagKind,
        end_kind: TagKind,
    },

    /// End tag without a corresponding start tag.
    ///
    /// ```plain
    /// Text
    /// EndGroup
    /// ```
    StartTagMissing {
        kind: TagKind,
    },

    /// Expected a specific start tag but instead is:
    /// - at the end of the document
    /// - at another start tag
    /// - at an end tag
    ExpectedStart {
        expected_start: TagKind,
        actual: ActualStart,
    },

    UnknownGroupId {
        group_id: GroupId,
    },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActualStart {
    /// The actual element is not a tag.
    Content,

    /// The actual element was a start tag of another kind.
    Start(TagKind),

    /// The actual element is an end tag instead of a start tag.
    End(TagKind),

    /// Reached the end of the document
    EndOfDocument,
}

impl std::fmt::Display for InvalidDocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidDocumentError::StartEndTagMismatch {
                start_kind,
                end_kind,
            } => {
                std::write!(
                    f,
                    "Expected end tag of kind {start_kind:?} but found {end_kind:?}."
                )
            }
            InvalidDocumentError::StartTagMissing { kind } => {
                std::write!(f, "End tag of kind {kind:?} without matching start tag.")
            }
            InvalidDocumentError::ExpectedStart {
                expected_start,
                actual,
            } => {
                match actual {
                    ActualStart::EndOfDocument => {
                        std::write!(f, "Expected start tag of kind {expected_start:?} but at the end of document.")
                    }
                    ActualStart::Start(start) => {
                        std::write!(f, "Expected start tag of kind {expected_start:?} but found start tag of kind {start:?}.")
                    }
                    ActualStart::End(end) => {
                        std::write!(f, "Expected start tag of kind {expected_start:?} but found end tag of kind {end:?}.")
                    }
                    ActualStart::Content => {
                        std::write!(f, "Expected start tag of kind {expected_start:?} but found non-tag element.")
                    }
                }
            }
            InvalidDocumentError::UnknownGroupId { group_id } => {
                std::write!(f, "Encountered unknown group id {group_id:?}. Ensure that the group with the id {group_id:?} exists and that the group is a parent of or comes before the element referring to it.")
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PrintError {
    InvalidDocument(InvalidDocumentError),
}

impl Error for PrintError {}

impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrintError::InvalidDocument(inner) => {
                std::write!(f, "Invalid document: {inner}")
            }
        }
    }
}
