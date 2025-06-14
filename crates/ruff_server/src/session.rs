//! Data model, state management, and configuration resolution.

use std::path::Path;
use std::sync::Arc;

use lsp_types::{ClientCapabilities, FileEvent, NotebookDocumentCellChange, Url};
use settings::ClientSettings;

use crate::edit::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::session::request_queue::RequestQueue;
use crate::session::settings::GlobalClientSettings;
use crate::workspace::Workspaces;
use crate::{PositionEncoding, TextDocument};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub use self::index::DocumentQuery;
pub(crate) use self::options::{AllOptions, WorkspaceOptionsMap};
pub use self::options::{ClientOptions, GlobalOptions};
pub use client::Client;

mod capabilities;
mod client;
mod index;
mod options;
mod request_queue;
mod settings;

/// The global state for the LSP
pub struct Session {
    /// Used to retrieve information about open documents and settings.
    index: index::Index,
    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,
    /// Global settings provided by the client.
    global_settings: GlobalClientSettings,

    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,

    /// Tracks the pending requests between client and server.
    request_queue: RequestQueue,

    /// Has the client requested the server to shutdown.
    shutdown_requested: bool,
}

/// An immutable snapshot of `Session` that references
/// a specific document.
pub struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    client_settings: Arc<settings::ClientSettings>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
}

impl Session {
    pub fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global: GlobalClientSettings,
        workspaces: &Workspaces,
        client: &Client,
    ) -> crate::Result<Self> {
        Ok(Self {
            position_encoding,
            index: index::Index::new(workspaces, &global, client)?,
            global_settings: global,
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
            request_queue: RequestQueue::new(),
            shutdown_requested: false,
        })
    }

    pub(crate) fn request_queue(&self) -> &RequestQueue {
        &self.request_queue
    }

    pub(crate) fn request_queue_mut(&mut self) -> &mut RequestQueue {
        &mut self.request_queue
    }

    pub(crate) fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    pub(crate) fn set_shutdown_requested(&mut self, requested: bool) {
        self.shutdown_requested = requested;
    }

    pub fn key_from_url(&self, url: Url) -> DocumentKey {
        self.index.key_from_url(url)
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        let key = self.key_from_url(url);
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            client_settings: self
                .index
                .client_settings(&key)
                .unwrap_or_else(|| self.global_settings.to_settings_arc()),
            document_ref: self.index.make_document_ref(key, &self.global_settings)?,
            position_encoding: self.position_encoding,
        })
    }

    /// Iterates over the LSP URLs for all open text documents. These URLs are valid file paths.
    pub(super) fn text_document_urls(&self) -> impl Iterator<Item = &lsp_types::Url> + '_ {
        self.index.text_document_urls()
    }

    /// Iterates over the LSP URLs for all open notebook documents. These URLs are valid file paths.
    pub(super) fn notebook_document_urls(&self) -> impl Iterator<Item = &lsp_types::Url> + '_ {
        self.index.notebook_document_urls()
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
    pub fn update_notebook_document(
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

    /// Registers a notebook document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub fn open_notebook_document(&mut self, url: Url, document: NotebookDocument) {
        self.index.open_notebook_document(url, document);
    }

    /// Registers a text document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, url: Url, document: TextDocument) {
        self.index.open_text_document(url, document);
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        self.index.close_document(key)?;
        Ok(())
    }

    /// Reloads the settings index based on the provided changes.
    pub(crate) fn reload_settings(&mut self, changes: &[FileEvent], client: &Client) {
        self.index.reload_settings(changes, client);
    }

    /// Open a workspace folder at the given `url`.
    pub(crate) fn open_workspace_folder(&mut self, url: Url, client: &Client) -> crate::Result<()> {
        self.index
            .open_workspace_folder(url, &self.global_settings, client)
    }

    /// Close a workspace folder at the given `url`.
    pub(crate) fn close_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.index.close_workspace_folder(url)?;
        Ok(())
    }

    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    /// Returns an iterator over the paths to the configuration files in the index.
    pub(crate) fn config_file_paths(&self) -> impl Iterator<Item = &Path> {
        self.index.config_file_paths()
    }

    /// Returns the resolved global client settings.
    pub(crate) fn global_client_settings(&self) -> &ClientSettings {
        self.global_settings.to_settings()
    }

    /// Returns the number of open documents in the session.
    pub(crate) fn open_documents_len(&self) -> usize {
        self.index.open_documents_len()
    }

    /// Returns an iterator over the workspace root folders in the session.
    pub(crate) fn workspace_root_folders(&self) -> impl Iterator<Item = &Path> {
        self.index.workspace_root_folders()
    }
}

impl DocumentSnapshot {
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn client_settings(&self) -> &settings::ClientSettings {
        &self.client_settings
    }

    pub fn query(&self) -> &index::DocumentQuery {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    /// Returns `true` if this snapshot represents a notebook cell.
    pub(crate) const fn is_notebook_cell(&self) -> bool {
        matches!(
            &self.document_ref,
            index::DocumentQuery::Notebook {
                cell_url: Some(_),
                ..
            }
        )
    }
}
