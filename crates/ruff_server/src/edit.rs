//! Types and utilities for working with text, modifying source files, and `Ruff <-> LSP` type conversion.

mod notebook;
mod range;
mod replacement;
mod text_document;

use std::collections::HashMap;

use lsp_types::{PositionEncodingKind, Url};
pub use notebook::NotebookDocument;
pub(crate) use range::{NotebookRange, RangeExt, ToRangeExt};
pub(crate) use replacement::Replacement;
pub use text_document::TextDocument;
pub(crate) use text_document::{DocumentVersion, LanguageId};

use crate::{fix::Fixes, session::ResolvedClientCapabilities};

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

/// A unique document ID, derived from a URL passed as part of an LSP request.
/// This document ID can point to either be a standalone Python file, a full notebook, or a cell within a notebook.
#[derive(Clone, Debug)]
pub enum DocumentKey {
    Notebook(Url),
    NotebookCell(Url),
    Text(Url),
}

impl DocumentKey {
    /// Converts the key back into its original URL.
    pub(crate) fn into_url(self) -> Url {
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

/// Tracks multi-document edits to eventually merge into a `WorkspaceEdit`.
/// Compatible with clients that don't support `workspace.workspaceEdit.documentChanges`.
#[derive(Debug)]
pub(crate) enum WorkspaceEditTracker {
    DocumentChanges(Vec<lsp_types::TextDocumentEdit>),
    Changes(HashMap<Url, Vec<lsp_types::TextEdit>>),
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

    /// Sets a series of [`Fixes`] for a text or notebook document.
    pub(crate) fn set_fixes_for_document(
        &mut self,
        fixes: Fixes,
        version: DocumentVersion,
    ) -> crate::Result<()> {
        for (uri, edits) in fixes {
            self.set_edits_for_document(uri, version, edits)?;
        }
        Ok(())
    }

    /// Sets the edits made to a specific document. This should only be called
    /// once for each document `uri`, and will fail if this is called for the same `uri`
    /// multiple times.
    pub(crate) fn set_edits_for_document(
        &mut self,
        uri: Url,
        _version: DocumentVersion,
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
                        // TODO(jane): Re-enable versioned edits after investigating whether it could work with notebook cells
                        version: None,
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
