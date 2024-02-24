//! Types and utilities for working with text, modifying source files, and `Ruff <-> LSP` type conversion.

mod document;
mod range;

pub use document::Document;
use lsp_types::PositionEncodingKind;
pub(crate) use range::{text_range, text_range_to_range};

/// A convenient enumeration for supported text encodings. Can be converted to [`lsp_types::PositionEncodingKind`].
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum PositionEncoding {
    /// UTF 16 is the encoding supported by all LSP clients.
    #[default]
    UTF16,

    /// Ruff's preferred encoding
    UTF8,

    /// Second choice because UTF32 uses a fixed 4 byte encoding for each character (makes conversion relatively easy)
    UTF32,
}

impl From<PositionEncoding> for lsp_types::PositionEncodingKind {
    fn from(value: PositionEncoding) -> Self {
        match value {
            PositionEncoding::UTF8 => lsp_types::PositionEncodingKind::UTF8,
            PositionEncoding::UTF16 => lsp_types::PositionEncodingKind::UTF16,
            PositionEncoding::UTF32 => lsp_types::PositionEncodingKind::UTF32,
        }
    }
}

impl TryFrom<lsp_types::PositionEncodingKind> for PositionEncoding {
    type Error = ();

    fn try_from(value: PositionEncodingKind) -> Result<Self, Self::Error> {
        Ok(if value == PositionEncodingKind::UTF8 {
            PositionEncoding::UTF8
        } else if value == PositionEncodingKind::UTF16 {
            PositionEncoding::UTF16
        } else if value == PositionEncodingKind::UTF32 {
            PositionEncoding::UTF32
        } else {
            return Err(());
        })
    }
}
