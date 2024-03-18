//! Data model, state management, and configuration resolution.

mod types;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{ops::Deref, sync::Arc};

use anyhow::anyhow;
use lsp_types::{ServerCapabilities, Url};
use ruff_workspace::resolver::{ConfigurationTransformer, Relativity};
use rustc_hash::FxHashMap;

use crate::edit::{Document, DocumentVersion};
use crate::PositionEncoding;

/// The global state for the LSP
pub(crate) struct Session {
    /// Workspace folders in the current session, which contain the state of all open files.
    workspaces: Workspaces,
    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,
    /// Extension-specific settings, set by the client, that apply to all workspace folders.
    #[allow(dead_code)]
    lsp_settings: types::ExtensionSettings,
}

/// An immutable snapshot of `Session` that references
/// a specific document.
pub(crate) struct DocumentSnapshot {
    configuration: Arc<RuffConfiguration>,
    document_ref: DocumentRef,
    position_encoding: PositionEncoding,
    url: Url,
}

#[derive(Default)]
pub(crate) struct RuffConfiguration {
    // settings to pass into the ruff linter
    pub(crate) linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
    pub(crate) formatter: ruff_workspace::FormatterSettings,
}

#[derive(Default)]
pub(crate) struct Workspaces(BTreeMap<PathBuf, Workspace>);

pub(crate) struct Workspace {
    open_documents: OpenDocuments,
    configuration: Arc<RuffConfiguration>,
}

#[derive(Default)]
pub(crate) struct OpenDocuments {
    documents: FxHashMap<Url, DocumentController>,
}

/// A mutable handler to an underlying document.
/// Handles copy-on-write mutation automatically when
/// calling `deref_mut`.
pub(crate) struct DocumentController {
    document: Arc<Document>,
}

/// A read-only reference to a document.
#[derive(Clone)]
pub(crate) struct DocumentRef {
    document: Arc<Document>,
}

impl Session {
    pub(crate) fn new(
        server_capabilities: &ServerCapabilities,
        workspaces: &[Url],
    ) -> crate::Result<Self> {
        Ok(Self {
            position_encoding: server_capabilities
                .position_encoding
                .as_ref()
                .and_then(|encoding| encoding.try_into().ok())
                .unwrap_or_default(),
            lsp_settings: types::ExtensionSettings,
            workspaces: Workspaces::new(workspaces)?,
        })
    }

    pub(crate) fn take_snapshot(&self, url: &Url) -> Option<DocumentSnapshot> {
        Some(DocumentSnapshot {
            configuration: self.workspaces.configuration(url)?.clone(),
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
    ) -> crate::Result<&mut DocumentController> {
        self.workspaces
            .controller(url)
            .ok_or_else(|| anyhow!("Tried to open unavailable document `{url}`"))
    }

    pub(crate) fn reload_configuration(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.reload_configuration(url)
    }

    pub(crate) fn open_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.open_workspace_folder(url)?;
        Ok(())
    }

    pub(crate) fn close_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.close_workspace_folder(url)?;
        Ok(())
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }
}

impl OpenDocuments {
    fn snapshot(&self, url: &Url) -> Option<DocumentRef> {
        Some(self.documents.get(url)?.make_ref())
    }

    fn controller(&mut self, url: &Url) -> Option<&mut DocumentController> {
        self.documents.get_mut(url)
    }

    fn open(&mut self, url: &Url, contents: String, version: DocumentVersion) {
        if self
            .documents
            .insert(url.clone(), DocumentController::new(contents, version))
            .is_some()
        {
            tracing::warn!("Opening document `{url}` that is already open!");
        }
    }

    fn close(&mut self, url: &Url) -> crate::Result<()> {
        let Some(_) = self.documents.remove(url) else {
            return Err(anyhow!(
                "Tried to close document `{url}`, which was not open"
            ));
        };
        Ok(())
    }
}

impl DocumentController {
    fn new(contents: String, version: DocumentVersion) -> Self {
        Self {
            document: Arc::new(Document::new(contents, version)),
        }
    }

    pub(crate) fn make_ref(&self) -> DocumentRef {
        DocumentRef {
            document: self.document.clone(),
        }
    }

    pub(crate) fn make_mut(&mut self) -> &mut Document {
        Arc::make_mut(&mut self.document)
    }
}

impl Deref for DocumentController {
    type Target = Document;
    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

impl Deref for DocumentRef {
    type Target = Document;
    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

impl DocumentSnapshot {
    pub(crate) fn configuration(&self) -> &RuffConfiguration {
        &self.configuration
    }

    pub(crate) fn document(&self) -> &DocumentRef {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub(crate) fn url(&self) -> &Url {
        &self.url
    }
}

impl Workspaces {
    fn new(urls: &[Url]) -> crate::Result<Self> {
        Ok(Self(
            urls.iter()
                .map(Workspace::new)
                .collect::<crate::Result<_>>()?,
        ))
    }

    fn open_workspace_folder(&mut self, folder_url: &Url) -> crate::Result<()> {
        let (path, workspace) = Workspace::new(folder_url)?;
        self.0.insert(path, workspace);
        Ok(())
    }

    fn close_workspace_folder(&mut self, folder_url: &Url) -> crate::Result<()> {
        let path = folder_url
            .to_file_path()
            .map_err(|()| anyhow!("Folder URI was not a proper file path"))?;
        self.0
            .remove(&path)
            .ok_or_else(|| anyhow!("Tried to remove non-existent folder {}", path.display()))?;
        Ok(())
    }

    fn snapshot(&self, document_url: &Url) -> Option<DocumentRef> {
        self.workspace_for_url(document_url)?
            .open_documents
            .snapshot(document_url)
    }

    fn controller(&mut self, document_url: &Url) -> Option<&mut DocumentController> {
        self.workspace_for_url_mut(document_url)?
            .open_documents
            .controller(document_url)
    }

    fn configuration(&self, document_url: &Url) -> Option<&Arc<RuffConfiguration>> {
        Some(&self.workspace_for_url(document_url)?.configuration)
    }

    fn reload_configuration(&mut self, changed_url: &Url) -> crate::Result<()> {
        let (path, workspace) = self
            .entry_for_url_mut(changed_url)
            .ok_or_else(|| anyhow!("Workspace not found for {changed_url}"))?;
        workspace.reload_configuration(path);
        Ok(())
    }

    fn open(&mut self, url: &Url, contents: String, version: DocumentVersion) {
        if let Some(workspace) = self.workspace_for_url_mut(url) {
            workspace.open_documents.open(url, contents, version);
        }
    }

    fn close(&mut self, url: &Url) -> crate::Result<()> {
        self.workspace_for_url_mut(url)
            .ok_or_else(|| anyhow!("Workspace not found for {url}"))?
            .open_documents
            .close(url)
    }

    fn workspace_for_url(&self, url: &Url) -> Option<&Workspace> {
        Some(self.entry_for_url(url)?.1)
    }

    fn workspace_for_url_mut(&mut self, url: &Url) -> Option<&mut Workspace> {
        Some(self.entry_for_url_mut(url)?.1)
    }

    fn entry_for_url(&self, url: &Url) -> Option<(&Path, &Workspace)> {
        let path = url.to_file_path().ok()?;
        self.0
            .range(..path)
            .next_back()
            .map(|(path, workspace)| (path.as_path(), workspace))
    }

    fn entry_for_url_mut(&mut self, url: &Url) -> Option<(&Path, &mut Workspace)> {
        let path = url.to_file_path().ok()?;
        self.0
            .range_mut(..path)
            .next_back()
            .map(|(path, workspace)| (path.as_path(), workspace))
    }
}

impl Workspace {
    pub(crate) fn new(root: &Url) -> crate::Result<(PathBuf, Self)> {
        let path = root
            .to_file_path()
            .map_err(|()| anyhow!("workspace URL was not a file path!"))?;
        // Fall-back to default configuration
        let configuration = Self::find_configuration_or_fallback(&path);

        Ok((
            path,
            Self {
                open_documents: OpenDocuments::default(),
                configuration: Arc::new(configuration),
            },
        ))
    }

    fn reload_configuration(&mut self, path: &Path) {
        self.configuration = Arc::new(Self::find_configuration_or_fallback(path));
    }

    fn find_configuration_or_fallback(root: &Path) -> RuffConfiguration {
        find_configuration_from_root(root).unwrap_or_else(|err| {
            tracing::error!("The following error occurred when trying to find a configuration file at `{}`:\n{err}", root.display());
            tracing::error!("Falling back to default configuration for `{}`", root.display());
            RuffConfiguration::default()
        })
    }
}

pub(crate) fn find_configuration_from_root(root: &Path) -> crate::Result<RuffConfiguration> {
    let pyproject = ruff_workspace::pyproject::find_settings_toml(root)?
        .ok_or_else(|| anyhow!("No pyproject.toml/ruff.toml/.ruff.toml file was found"))?;
    let settings = ruff_workspace::resolver::resolve_root_settings(
        &pyproject,
        Relativity::Parent,
        &LSPConfigTransformer,
    )?;
    Ok(RuffConfiguration {
        linter: settings.linter,
        formatter: settings.formatter,
    })
}

struct LSPConfigTransformer;

impl ConfigurationTransformer for LSPConfigTransformer {
    fn transform(
        &self,
        config: ruff_workspace::configuration::Configuration,
    ) -> ruff_workspace::configuration::Configuration {
        config
    }
}
