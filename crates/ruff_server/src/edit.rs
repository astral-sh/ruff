//! Types and utilities for working with text, modifying source files, and `Ruff <-> LSP` type conversion.

mod document;
mod range;
mod replacement;

use std::collections::HashMap;

pub use document::Document;
pub(crate) use document::DocumentVersion;
use lsp_types::PositionEncodingKind;
pub(crate) use range::{RangeExt, ToRangeExt};
pub(crate) use replacement::Replacement;

use crate::session::ResolvedClientCapabilities;

/// A convenient enumeration for supported text encodings. Can be converted to [`lsp_types::PositionEncodingKind`].
// Please maintain the order from least to greatest priority for the derived `Ord` impl.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PositionEncoding {
    /// UTF 16 is the encoding supported by all LSP clients.
    #[default]
    UTF16,

    /// Second choice because UTF32 uses a fixed 4 byte encoding for each character (makes conversion relatively easy)
    UTF32,

    /// Ruff's preferred encoding
    UTF8,
}

/// Tracks multi-document edits to eventually merge into a `WorkspaceEdit`.
/// Compatible with clients that don't support `workspace.workspaceEdit.documentChanges`.
#[derive(Debug)]
pub(crate) enum WorkspaceEditTracker {
    DocumentChanges(Vec<lsp_types::TextDocumentEdit>),
    Changes(HashMap<lsp_types::Url, Vec<lsp_types::TextEdit>>),
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

impl TryFrom<&lsp_types::PositionEncodingKind> for PositionEncoding {
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

impl WorkspaceEditTracker {
    pub(crate) fn new(client_capabilities: &ResolvedClientCapabilities) -> Self {
        if client_capabilities.document_changes {
            Self::DocumentChanges(Vec::default())
        } else {
            Self::Changes(HashMap::default())
        }
    }

    /// Sets the edits made to a specific document. This should only be called
    /// once for each document `uri`, and will fail if this is called for the same `uri`
    /// multiple times.
    pub(crate) fn set_edits_for_document(
        &mut self,
        uri: lsp_types::Url,
        version: DocumentVersion,
        edits: Vec<lsp_types::TextEdit>,
    ) -> crate::Result<()> {
        match self {
            Self::DocumentChanges(document_edits) => {
                if document_edits
                    .iter()
                    .any(|document| document.text_document.uri == uri)
                {
                    return Err(anyhow::anyhow!(
                        "Attempted to add edits for a document that was already edited"
                    ));
                }
                document_edits.push(lsp_types::TextDocumentEdit {
                    text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                        uri,
                        version: Some(version),
                    },
                    edits: edits.into_iter().map(lsp_types::OneOf::Left).collect(),
                });
                Ok(())
            }
            Self::Changes(changes) => {
                if changes.get(&uri).is_some() {
                    return Err(anyhow::anyhow!(
                        "Attempted to add edits for a document that was already edited"
                    ));
                }
                changes.insert(uri, edits);
                Ok(())
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::DocumentChanges(document_edits) => document_edits.is_empty(),
            Self::Changes(changes) => changes.is_empty(),
        }
    }

    pub(crate) fn into_workspace_edit(self) -> lsp_types::WorkspaceEdit {
        match self {
            Self::DocumentChanges(document_edits) => lsp_types::WorkspaceEdit {
                document_changes: Some(lsp_types::DocumentChanges::Edits(document_edits)),
                ..Default::default()
            },
            Self::Changes(changes) => lsp_types::WorkspaceEdit::new(changes),
        }
    }
}
