//! Data model, state management, and configuration resolution.

mod capabilities;
mod index;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, NotebookDocumentCellChange, Url};

use crate::edit::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::{PositionEncoding, TextDocument};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub(crate) use self::index::DocumentQuery;
pub(crate) use self::settings::{AllSettings, ClientSettings};

/// The global state for the LSP
pub(crate) struct Session {
    /// Used to retrieve information about open documents and settings.
    index: index::Index,
    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,
    /// Global settings provided by the client.
    global_settings: ClientSettings,
    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
}

/// An immutable snapshot of `Session` that references
/// a specific document.
pub(crate) struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    client_settings: settings::ResolvedClientSettings,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
}

impl Session {
    pub(crate) fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_settings: ClientSettings,
        workspace_folders: Vec<(PathBuf, ClientSettings)>,
    ) -> Self {
        Self {
            position_encoding,
            index: index::Index::new(workspace_folders, &global_settings),
            global_settings,
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
        }
    }

    pub(crate) fn key_from_url(&self, url: &lsp_types::Url) -> crate::Result<DocumentKey> {
        self.index
            .key_from_url(url)
            .ok_or_else(|| anyhow!("No document found for {url}"))
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub(crate) fn take_snapshot(&self, url: &Url) -> Option<DocumentSnapshot> {
        let key = self.key_from_url(url).ok()?;
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            client_settings: self.index.client_settings(&key, &self.global_settings),
            document_ref: self.index.make_document_ref(key)?,
            position_encoding: self.position_encoding,
        })
    }

    /// Updates a text document at the associated `key`.
    ///
    /// The document key must point to a text document, or this will throw an error.
    pub(crate) fn update_text_document(
        &mut self,
        key: &DocumentKey,
        content_changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
    ) -> crate::Result<()> {
        let encoding = self.encoding();

        self.index
            .update_text_document(key, content_changes, new_version, encoding)
    }

    /// Updates a notebook document at the associated `key` with potentially new
    /// cell, metadata, and version values.
    ///
    /// The document key must point to a notebook document or cell, or this will
    /// throw an error.
    pub(crate) fn update_notebook_document(
        &mut self,
        key: &DocumentKey,
        cells: Option<NotebookDocumentCellChange>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
        version: DocumentVersion,
    ) -> crate::Result<()> {
        let encoding = self.encoding();
        self.index
            .update_notebook_document(key, cells, metadata, version, encoding)
    }

    /// Registers a notebook document at the provided `path`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_notebook_document(&mut self, path: PathBuf, document: NotebookDocument) {
        self.index.open_notebook_document(path, document);
    }

    /// Registers a text document at the provided `path`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, path: PathBuf, document: TextDocument) {
        self.index.open_text_document(path, document);
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        self.index.close_document(key)?;
        Ok(())
    }

    /// Reloads the settings index
    pub(crate) fn reload_settings(&mut self, changed_path: &PathBuf) {
        self.index.reload_settings(changed_path);
    }

    /// Open a workspace folder at the given `path`.
    pub(crate) fn open_workspace_folder(&mut self, path: PathBuf) {
        self.index
            .open_workspace_folder(path, &self.global_settings);
    }

    /// Close a workspace folder at the given `path`.
    pub(crate) fn close_workspace_folder(&mut self, path: &PathBuf) -> crate::Result<()> {
        self.index.close_workspace_folder(path)?;
        Ok(())
    }

    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }
}

impl DocumentSnapshot {
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn client_settings(&self) -> &settings::ResolvedClientSettings {
        &self.client_settings
    }

    pub(crate) fn query(&self) -> &index::DocumentQuery {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }
}
