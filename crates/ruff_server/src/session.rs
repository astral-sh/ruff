//! Data model, state management, and configuration resolution.

use std::path::Path;
use std::sync::Arc;

use lsp_types::{ClientCapabilities, FileEvent, NotebookDocumentCellChanges, Uri};
use settings::ClientSettings;

use crate::edit::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::format::FormatBackend;
use crate::session::request_queue::RequestQueue;
use crate::session::settings::GlobalClientSettings;
use crate::workspace::Workspaces;
use crate::{PositionEncoding, TextDocument, WorkspaceTrust};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub(crate) use self::index::DocumentQuery;
pub(crate) use self::options::ClientOptions;
pub(crate) use self::options::{AllOptions, WorkspaceOptionsMap};
pub(crate) use client::Client;

mod capabilities;
mod client;
mod index;
mod options;
mod request_queue;
mod settings;

/// The global state for the LSP
pub(crate) struct Session {
    /// Used to retrieve information about open documents and settings.
    index: index::Index,
    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,
    /// Global settings provided by the client.
    global_settings: GlobalClientSettings,

    /// Set at startup so client settings cannot enable external formatting in an untrusted workspace.
    workspace_trust: WorkspaceTrust,

    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,

    /// Tracks the pending requests between client and server.
    request_queue: RequestQueue,

    /// Has the client requested the server to shutdown.
    shutdown_requested: bool,
}

/// An immutable snapshot of `Session` that references
/// a specific document.
pub(crate) struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    client_settings: Arc<settings::ClientSettings>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
    workspace_trust: WorkspaceTrust,
}

impl Session {
    pub(crate) fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global: GlobalClientSettings,
        workspaces: &Workspaces,
        client: &Client,
        workspace_trust: WorkspaceTrust,
    ) -> crate::Result<Self> {
        Ok(Self {
            position_encoding,
            index: index::Index::new(workspaces, &global, client)?,
            global_settings: global,
            workspace_trust,
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

    pub(crate) fn key_from_uri(&self, uri: Uri) -> DocumentKey {
        self.index.key_from_uri(uri)
    }

    /// Creates a document snapshot with the URI referencing the document to snapshot.
    pub(crate) fn take_snapshot(&self, uri: Uri) -> Option<DocumentSnapshot> {
        let key = self.key_from_uri(uri);
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            client_settings: self
                .index
                .client_settings(&key)
                .unwrap_or_else(|| self.global_settings.to_settings_arc()),
            document_ref: self.index.make_document_ref(key, &self.global_settings)?,
            position_encoding: self.position_encoding,
            workspace_trust: self.workspace_trust,
        })
    }

    /// Iterates over the LSP URIs for all open text documents. These URIs are valid file paths.
    pub(super) fn text_document_uris(&self) -> impl Iterator<Item = &Uri> + '_ {
        self.index.text_document_uris()
    }

    /// Iterates over the LSP URIs for all open notebook documents. These URIs are valid file paths.
    pub(super) fn notebook_document_uris(&self) -> impl Iterator<Item = &Uri> + '_ {
        self.index.notebook_document_uris()
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
        cells: Option<NotebookDocumentCellChanges>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
        version: DocumentVersion,
    ) -> crate::Result<()> {
        let encoding = self.encoding();
        self.index
            .update_notebook_document(key, cells, metadata, version, encoding)
    }

    /// Registers a notebook document at the provided `uri`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_notebook_document(&mut self, uri: Uri, document: NotebookDocument) {
        self.index.open_notebook_document(uri, document);
    }

    /// Registers a text document at the provided `uri`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, uri: Uri, document: TextDocument) {
        self.index.open_text_document(uri, document);
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

    /// Open a workspace folder at the given `uri`.
    pub(crate) fn open_workspace_folder(&mut self, uri: Uri, client: &Client) -> crate::Result<()> {
        self.index
            .open_workspace_folder(uri, &self.global_settings, client)
    }

    /// Close a workspace folder at the given `uri`.
    pub(crate) fn close_workspace_folder(&mut self, uri: &Uri) -> crate::Result<()> {
        self.index.close_workspace_folder(uri)?;
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
    pub(crate) fn format_backend(&self) -> FormatBackend {
        let backend = self.client_settings.editor_settings().format_backend();
        match self.workspace_trust {
            WorkspaceTrust::Trusted => backend,
            WorkspaceTrust::Untrusted => {
                if backend == FormatBackend::Uv {
                    tracing::info!(
                        "Using the internal formatter because the workspace is untrusted; the uv backend is disabled"
                    );
                }
                FormatBackend::Internal
            }
        }
    }

    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn client_settings(&self) -> &settings::ClientSettings {
        &self.client_settings
    }

    pub(crate) fn query(&self) -> &index::DocumentQuery {
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
                cell_uri: Some(_),
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::{Context, Result};
    use lsp_types::Uri;
    use serde_json::json;
    use test_case::test_case;

    use super::index::Index;
    use super::options::GlobalOptions;
    use super::{Client, Session};
    use crate::format::FormatBackend;
    use crate::session::request_queue::RequestQueue;
    use crate::{PositionEncoding, TextDocument, WorkspaceTrust};

    #[test_case(WorkspaceTrust::Untrusted, FormatBackend::Internal)]
    #[test_case(WorkspaceTrust::Trusted, FormatBackend::Uv)]
    fn format_backend(workspace_trust: WorkspaceTrust, expected: FormatBackend) -> Result<()> {
        let (main_loop_sender, _) = crossbeam::channel::unbounded();
        let (client_sender, _) = crossbeam::channel::unbounded();
        let client = Client::new(main_loop_sender, client_sender);
        let options: GlobalOptions = serde_json::from_value(json!({
            "format": {"backend": "uv"},
        }))?;
        let mut session = Session {
            index: Index::default(),
            position_encoding: PositionEncoding::default(),
            global_settings: options.into_settings(client),
            workspace_trust,
            resolved_client_capabilities: Arc::default(),
            request_queue: RequestQueue::new(),
            shutdown_requested: false,
        };
        let uri: Uri = "untitled:test.py".parse()?;
        session.open_text_document(uri.clone(), TextDocument::new(String::new(), 1));
        let snapshot = session
            .take_snapshot(uri)
            .context("missing document snapshot")?;

        assert_eq!(snapshot.format_backend(), expected);

        Ok(())
    }
}
