//! Types and utilities for working with text, modifying source files, and `ty <-> LSP` type conversion.

mod location;
mod notebook;
mod range;
mod text_document;

use lsp_types::{PositionEncodingKind, Url};
use ruff_db::system::{SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf};

use crate::system::AnySystemPath;
pub(crate) use location::ToLink;
pub use notebook::NotebookDocument;
pub(crate) use range::{FileRangeExt, PositionExt, RangeExt, TextSizeExt, ToRangeExt};
pub use text_document::TextDocument;
pub(crate) use text_document::{DocumentVersion, LanguageId};

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
///
/// The `DocumentKey` is very similar to `AnySystemPath`. The important distinction is that
/// ty doesn't know about individual notebook cells, instead, ty operates on full notebook documents.
/// ty also doesn't support resolving settings per cell, instead, settings are resolved per file or notebook.
///
/// Thus, the motivation of `DocumentKey` is to prevent accidental use of Cell keys for operations
/// that expect to work on a file path level. That's what [`DocumentHandle::to_file_path`]
/// is for, it returns a file path for any document, taking into account that these methods should
/// return the notebook for cell documents and notebooks.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum DocumentKey {
    /// A URI using the `file` schema and maps to a valid path.
    File(SystemPathBuf),

    /// Any other URI.
    ///
    /// Used for Notebook-cells, URI's with non-`file` schemes, or invalid `file` URI's.
    Opaque(String),
}

impl DocumentKey {
    /// Converts the given [`Url`] to an [`DocumentKey`].
    ///
    /// If the URL scheme is `file`, then the path is converted to a [`SystemPathBuf`] unless
    /// the url isn't a valid file path.
    ///
    /// In all other cases, the URL is kept as an opaque identifier ([`Self::Opaque`]).
    pub(crate) fn from_url(url: &Url) -> Self {
        if url.scheme() == "file" {
            if let Ok(path) = url.to_file_path() {
                Self::File(SystemPathBuf::from_path_buf(path).expect("URL to be valid UTF-8"))
            } else {
                tracing::warn!(
                    "Treating `file:` url `{url}` as opaque URL as it isn't a valid file path"
                );
                Self::Opaque(url.to_string())
            }
        } else {
            Self::Opaque(url.to_string())
        }
    }

    /// Returns the corresponding [`AnySystemPath`] for this document key.
    ///
    /// Note, calling this method on a `DocumentKey::Opaque` representing a cell document
    /// will return a `SystemVirtualPath` corresponding to the cell URI but not the notebook file path.
    /// That's most likely not what you want.
    pub(super) fn to_file_path(&self) -> AnySystemPath {
        match self {
            Self::File(path) => AnySystemPath::System(path.clone()),
            Self::Opaque(uri) => {
                AnySystemPath::SystemVirtual(SystemVirtualPath::new(uri).to_path_buf())
            }
        }
    }

    pub(super) fn into_file_path(self) -> AnySystemPath {
        match self {
            Self::File(path) => AnySystemPath::System(path),
            Self::Opaque(uri) => AnySystemPath::SystemVirtual(SystemVirtualPathBuf::from(uri)),
        }
    }
}

impl From<AnySystemPath> for DocumentKey {
    fn from(value: AnySystemPath) -> Self {
        match value {
            AnySystemPath::System(system_path) => Self::File(system_path),
            AnySystemPath::SystemVirtual(virtual_path) => Self::Opaque(virtual_path.to_string()),
        }
    }
}

impl std::fmt::Display for DocumentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(path) => path.fmt(f),
            Self::Opaque(uri) => uri.fmt(f),
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
