//! Types and utilities for working with text, modifying source files, and `ty <-> LSP` type conversion.

mod location;
mod notebook;
mod range;
mod text_document;

pub(crate) use location::ToLink;
use lsp_types::{PositionEncodingKind, Url};
pub use notebook::NotebookDocument;
pub(crate) use range::{FileRangeExt, PositionExt, RangeExt, TextSizeExt, ToRangeExt};
pub(crate) use text_document::DocumentVersion;
pub use text_document::TextDocument;

/// A convenient enumeration for supported text encodings. Can be converted to [`lsp_types::PositionEncodingKind`].
// Please maintain the order from least to greatest priority for the derived `Ord` impl.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PositionEncoding {
    /// UTF 16 is the encoding supported by all LSP clients.
    #[default]
    UTF16,

    /// Second choice because UTF32 uses a fixed 4 byte encoding for each character (makes conversion relatively easy)
    UTF32,

    /// ty's preferred encoding
    UTF8,
}

impl From<PositionEncoding> for ruff_source_file::PositionEncoding {
    fn from(value: PositionEncoding) -> Self {
        match value {
            PositionEncoding::UTF8 => Self::Utf8,
            PositionEncoding::UTF16 => Self::Utf16,
            PositionEncoding::UTF32 => Self::Utf32,
        }
    }
}

/// A unique document ID, derived from a URL passed as part of an LSP request.
/// This document ID can point to either be a standalone Python file, a full notebook, or a cell within a notebook.
#[derive(Clone, Debug)]
pub enum DocumentKey {
    Notebook(Url),
    NotebookCell(Url),
    Text(Url),
}

impl DocumentKey {
    /// Returns the URL associated with the key.
    pub(crate) fn url(&self) -> &Url {
        match self {
            DocumentKey::NotebookCell(url)
            | DocumentKey::Notebook(url)
            | DocumentKey::Text(url) => url,
        }
    }
}

impl std::fmt::Display for DocumentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotebookCell(url) | Self::Notebook(url) | Self::Text(url) => url.fmt(f),
        }
    }
}

impl From<PositionEncoding> for PositionEncodingKind {
    fn from(value: PositionEncoding) -> Self {
        match value {
            PositionEncoding::UTF8 => PositionEncodingKind::UTF8,
            PositionEncoding::UTF16 => PositionEncodingKind::UTF16,
            PositionEncoding::UTF32 => PositionEncodingKind::UTF32,
        }
    }
}

impl TryFrom<&PositionEncodingKind> for PositionEncoding {
    type Error = ();

    fn try_from(value: &PositionEncodingKind) -> Result<Self, Self::Error> {
        Ok(if value == &PositionEncodingKind::UTF8 {
            PositionEncoding::UTF8
        } else if value == &PositionEncodingKind::UTF16 {
            PositionEncoding::UTF16
        } else if value == &PositionEncodingKind::UTF32 {
            PositionEncoding::UTF32
        } else {
            return Err(());
        })
    }
}
