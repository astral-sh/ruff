//! Data model, state management, and configuration resolution.

mod types;

use std::collections::BTreeMap;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::{ops::Deref, sync::Arc};

use anyhow::anyhow;
use crossbeam::channel::{unbounded, Receiver};
use lsp_types::{ServerCapabilities, Url};
use notify::Watcher;
use ruff_workspace::resolver::{ConfigurationTransformer, Relativity};
use rustc_hash::FxHashMap;

use crate::edit::Document;
use crate::PositionEncoding;

/// The global state for the LSP.
pub(crate) struct Session {
    workspaces: Workspaces,
    position_encoding: PositionEncoding,
    #[allow(dead_code)]
    lsp_settings: types::Settings,
    watcher: notify::RecommendedWatcher,
    watch_recv: Receiver<notify::Result<notify::Event>>,
}

/// An immutable snapshot of `Session` that references
/// a specific document.
pub(crate) struct SessionSnapshot {
    configuration: Arc<Configuration>,
    document_ref: DocumentRef,
    position_encoding: PositionEncoding,
    url: Url,
}

#[derive(Default)]
pub(crate) struct Configuration {
    // settings to pass into the ruff linter
    pub(crate) linter: ruff_linter::settings::LinterSettings,
    // settings to pass into the ruff formatter
    pub(crate) formatter: ruff_workspace::FormatterSettings,
}

#[derive(Default)]
pub(crate) struct Workspaces(BTreeMap<PathBuf, Workspace>);

pub(crate) struct Workspace {
    open_documents: OpenDocuments,
    configuration: Arc<Configuration>,
}

#[derive(Default)]
pub(crate) struct OpenDocuments {
    documents: FxHashMap<Url, DocumentController>,
}

/// A handler to an underlying document, with a revision counter.
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
        let (tx, rx) = unbounded();
        let mut watcher = notify::recommended_watcher(tx)?;
        let paths: Result<Vec<PathBuf>, _> = workspaces.iter().map(Url::to_file_path).collect();
        for url in paths.map_err(|()| anyhow!("Workspace URL was not a valid file path"))? {
            watcher.watch(&url, notify::RecursiveMode::Recursive)?;
        }
        Ok(Self {
            position_encoding: server_capabilities
                .position_encoding
                .clone()
                .and_then(|encoding| encoding.try_into().ok())
                .unwrap_or_default(),
            lsp_settings: types::Settings,
            workspaces: Workspaces::new(workspaces)?,
            watcher,
            watch_recv: rx,
        })
    }
    pub(crate) fn take_snapshot(&self, url: &Url) -> Option<SessionSnapshot> {
        Some(SessionSnapshot {
            configuration: self.workspaces.configuration(url)?.clone(),
            document_ref: self.workspaces.doc_snapshot(url)?,
            position_encoding: self.position_encoding,
            url: url.clone(),
        })
    }

    pub(crate) fn open_document(&mut self, url: &Url, contents: String, version: i32) {
        self.workspaces.open_document(url, contents, version);
    }

    pub(crate) fn close_document(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.close_document(url)?;
        Ok(())
    }

    pub(crate) fn document_controller(
        &mut self,
        url: &Url,
    ) -> crate::Result<&mut DocumentController> {
        self.workspaces
            .doc_controller(url)
            .ok_or_else(|| anyhow!("Tried to open unavailable document `{url}`"))
    }

    /// Processes any file changes made since the last call and forwards each event
    /// to the appropriate workspace, in the order that they were received.
    /// Returns `true` if at least one configuration file was changed in at least
    /// one workspace.
    pub(crate) fn update_configuration_files(&mut self) -> bool {
        let mut configuration_changed = false;
        while let Ok(event) = self.watch_recv.try_recv() {
            match event {
                Ok(event) => {
                    configuration_changed |= self.workspaces.update_configuration_files(&event);
                }
                Err(err) => {
                    tracing::error!("An error occured with the workspace file watcher:\n{err}");
                }
            }
        }
        configuration_changed
    }

    pub(crate) fn open_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.open_workspace_folder(url)?;
        self.track_url(url);
        Ok(())
    }

    pub(crate) fn close_workspace_folder(&mut self, url: &Url) -> crate::Result<()> {
        self.workspaces.close_workspace_folder(url)?;
        self.stop_tracking_url(url);
        Ok(())
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    fn track_url(&mut self, url: &Url) {
        if let Ok(path) = url.to_file_path() {
            // TODO(jane): report error here
            let _ = self.watcher.watch(&path, notify::RecursiveMode::Recursive);
        }
    }

    fn stop_tracking_url(&mut self, url: &Url) {
        if let Ok(path) = url.to_file_path() {
            // TODO(jane): report error here
            let _ = self.watcher.unwatch(&path);
        }
    }
}

impl OpenDocuments {
    fn doc_snapshot(&self, url: &Url) -> Option<DocumentRef> {
        Some(self.documents.get(url)?.make_ref())
    }
    fn doc_controller(&mut self, url: &Url) -> Option<&mut DocumentController> {
        self.documents.get_mut(url)
    }
    fn open_document(&mut self, url: &Url, contents: String, version: i32) {
        if self
            .documents
            .insert(url.clone(), DocumentController::new(contents, version))
            .is_some()
        {
            tracing::warn!("Opening document `{url}` that is already open!");
        }
    }
    fn close_document(&mut self, url: &Url) -> crate::Result<()> {
        let Some(_) = self.documents.remove(url) else {
            return Err(anyhow!(
                "Tried to close document `{url}`, which was not open"
            ));
        };
        Ok(())
    }
}

impl DocumentController {
    fn new(contents: String, version: i32) -> Self {
        Self {
            document: Arc::new(Document::new(contents, version)),
        }
    }
    fn make_ref(&self) -> DocumentRef {
        DocumentRef {
            document: self.document.clone(),
        }
    }
}

impl Deref for DocumentController {
    type Target = Document;
    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

impl DerefMut for DocumentController {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.document)
    }
}

impl Deref for DocumentRef {
    type Target = Document;
    fn deref(&self) -> &Self::Target {
        &self.document
    }
}

impl SessionSnapshot {
    pub(crate) fn configuration(&self) -> &Configuration {
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

    fn update_configuration_files(&mut self, event: &notify::Event) -> bool {
        for path in &event.paths {
            if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("ruff.toml" | "pyproject.toml")
            ) {
                continue;
            }
            if let Some((workspace_path, workspace)) = self.mut_entry_for_path(path) {
                workspace.reload_configuration(&workspace_path);
                return true;
            }
        }
        false
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

    fn doc_snapshot(&self, document_url: &Url) -> Option<DocumentRef> {
        self.workspace_for_url(document_url)
            .and_then(|w| w.open_documents.doc_snapshot(document_url))
    }

    fn doc_controller(&mut self, document_url: &Url) -> Option<&mut DocumentController> {
        self.mut_workspace_for_url(document_url)
            .and_then(|w| w.open_documents.doc_controller(document_url))
    }

    fn configuration(&self, document_url: &Url) -> Option<&Arc<Configuration>> {
        self.workspace_for_url(document_url)
            .map(|w| &w.configuration)
    }

    fn open_document(&mut self, url: &Url, contents: String, version: i32) {
        if let Some(w) = self.mut_workspace_for_url(url) {
            w.open_documents.open_document(url, contents, version);
        }
    }

    fn close_document(&mut self, url: &Url) -> crate::Result<()> {
        self.mut_workspace_for_url(url)
            .ok_or_else(|| anyhow!("Workspace not found for {url}"))?
            .open_documents
            .close_document(url)
    }

    fn workspace_for_url(&self, url: &Url) -> Option<&Workspace> {
        let path = url.to_file_path().ok()?;
        self.0
            .keys()
            .filter(|p| path.starts_with(p))
            .max_by_key(|p| p.as_os_str().len())
            .and_then(|u| self.0.get(u))
    }

    fn mut_workspace_for_url(&mut self, url: &Url) -> Option<&mut Workspace> {
        let path = url.to_file_path().ok()?;
        self.0
            .keys()
            .filter(|p| path.starts_with(p))
            .max_by_key(|p| p.as_os_str().len())
            .cloned()
            .and_then(|u| self.0.get_mut(&u))
    }

    fn mut_entry_for_path(&mut self, path: &Path) -> Option<(PathBuf, &mut Workspace)> {
        self.0
            .keys()
            .filter(|p| path.starts_with(p))
            .max_by_key(|p| p.as_os_str().len())
            .cloned()
            .and_then(|u| {
                let workspace = self.0.get_mut(&u)?;
                Some((u, workspace))
            })
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

    fn find_configuration_or_fallback(root: &Path) -> Configuration {
        find_configuration_from_root(root).unwrap_or_else(|err| {
            tracing::error!("The following error occured when trying to find a configuration file at `{}`:\n{err}", root.display());
            tracing::error!("Falling back to default configuration for `{}`", root.display());
            Configuration::default()
        })
    }
}

pub(crate) fn find_configuration_from_root(root: &Path) -> crate::Result<Configuration> {
    let pyproject = ruff_workspace::pyproject::find_settings_toml(root)?
        .ok_or_else(|| anyhow!("No pyproject.toml/ruff.toml file was found"))?;
    let settings = ruff_workspace::resolver::resolve_root_settings(
        &pyproject,
        Relativity::Parent,
        &LSPConfigTransformer,
    )?;
    Ok(Configuration {
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
