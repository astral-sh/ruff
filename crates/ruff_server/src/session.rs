//! Data model, state management, and configuration resolution.

mod capabilities;
mod settings;
mod workspace;

use std::sync::Arc;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, Url};

use crate::edit::DocumentVersion;
use crate::PositionEncoding;

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub(crate) use self::settings::{AllSettings, ClientSettings};

/// The global state for the LSP
pub(crate) struct Session {
    /// Workspace folders in the current session, which contain the state of all open files.
    workspaces: workspace::Workspaces,
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
    document_ref: workspace::DocumentRef,
    position_encoding: PositionEncoding,
    url: Url,
}

impl Session {
    pub(crate) fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_settings: ClientSettings,
        workspaces: Vec<(Url, ClientSettings)>,
    ) -> crate::Result<Self> {
        Ok(Self {
            position_encoding,
            workspaces: workspace::Workspaces::new(workspaces, &global_settings)?,
            global_settings,
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
        })
    }

    pub(crate) fn take_snapshot(&self, url: &Url) -> Option<DocumentSnapshot> {
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            client_settings: self.workspaces.client_settings(url, &self.global_settings),
            document_ref: self.workspaces.snapshot(url)?,
            position_encoding: self.position_encoding,
            url: url.clone(),
        })
    }

    pub(crate) fn open_document(&mut self, url: &Url, contents: String, version: DocumentVersion) {
        self.workspaces.open(url, contents, version);
    }

    pub(crate) fn close_document(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.close(url)?;
        Ok(())
    }

    pub(crate) fn document_controller(
        &mut self,
        url: &Url,
    ) -> crate::Result<&mut workspace::DocumentController> {
        self.workspaces
            .controller(url)
            .ok_or_else(|| anyhow!("Tried to open unavailable document `{url}`"))
    }

    pub(crate) fn reload_settings(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.reload_settings(url)
    }

    pub(crate) fn open_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces
            .open_workspace_folder(url, &self.global_settings)?;
        Ok(())
    }

    pub(crate) fn close_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.close_workspace_folder(url)?;
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
    pub(crate) fn settings(&self) -> &workspace::RuffSettings {
        self.document().settings()
    }

    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn client_settings(&self) -> &settings::ResolvedClientSettings {
        &self.client_settings
    }

    pub(crate) fn document(&self) -> &workspace::DocumentRef {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub(crate) fn url(&self) -> &Url {
        &self.url
    }
}
