use anyhow::anyhow;
use lsp_types::Url;
use rustc_hash::FxHashMap;
use std::{
    collections::BTreeMap,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{edit::DocumentVersion, Document};

use super::{settings, ClientSettings};

mod configuration;

pub(crate) use configuration::RuffConfiguration;

#[derive(Default)]
pub(crate) struct Workspaces(BTreeMap<PathBuf, Workspace>);

pub(crate) struct Workspace {
    open_documents: OpenDocuments,
    settings: ClientSettings,
}

#[derive(Default)]
pub(crate) struct OpenDocuments {
    documents: FxHashMap<Url, DocumentController>,
    configuration_index: configuration::ConfigurationIndex,
}

/// A mutable handler to an underlying document.
/// Handles copy-on-write mutation automatically when
/// calling `deref_mut`.
pub(crate) struct DocumentController {
    document: Arc<Document>,
    configuration: Arc<RuffConfiguration>,
}

/// A read-only reference to a document.
#[derive(Clone)]
pub(crate) struct DocumentRef {
    document: Arc<Document>,
    configuration: Arc<RuffConfiguration>,
}

impl Workspaces {
    pub(super) fn new(workspaces: Vec<(Url, ClientSettings)>) -> crate::Result<Self> {
        Ok(Self(
            workspaces
                .into_iter()
                .map(|(url, settings)| Workspace::new(&url, settings))
                .collect::<crate::Result<_>>()?,
        ))
    }

    pub(super) fn open_workspace_folder(&mut self, folder_url: &Url) -> crate::Result<()> {
        // TODO(jane): find a way to allow for workspace settings to be updated dynamically
        let (path, workspace) = Workspace::new(folder_url, ClientSettings::default())?;
        self.0.insert(path, workspace);
        Ok(())
    }

    pub(super) fn close_workspace_folder(&mut self, folder_url: &Url) -> crate::Result<()> {
        let path = folder_url
            .to_file_path()
            .map_err(|()| anyhow!("Folder URI was not a proper file path"))?;
        self.0
            .remove(&path)
            .ok_or_else(|| anyhow!("Tried to remove non-existent folder {}", path.display()))?;
        Ok(())
    }

    pub(super) fn snapshot(&self, document_url: &Url) -> Option<DocumentRef> {
        self.workspace_for_url(document_url)?
            .open_documents
            .snapshot(document_url)
    }

    pub(super) fn controller(&mut self, document_url: &Url) -> Option<&mut DocumentController> {
        self.workspace_for_url_mut(document_url)?
            .open_documents
            .controller(document_url)
    }

    pub(super) fn reload_configuration(&mut self, changed_url: &Url) -> crate::Result<()> {
        let workspace = self
            .workspace_for_url_mut(changed_url)
            .ok_or_else(|| anyhow!("Workspace not found for {changed_url}"))?;
        workspace.reload_configuration();
        Ok(())
    }

    pub(super) fn open(&mut self, url: &Url, contents: String, version: DocumentVersion) {
        if let Some(workspace) = self.workspace_for_url_mut(url) {
            workspace.open_documents.open(url, contents, version);
        }
    }

    pub(super) fn close(&mut self, url: &Url) -> crate::Result<()> {
        self.workspace_for_url_mut(url)
            .ok_or_else(|| anyhow!("Workspace not found for {url}"))?
            .open_documents
            .close(url)
    }

    pub(super) fn client_settings(
        &self,
        url: &Url,
        global_settings: &ClientSettings,
    ) -> settings::ResolvedClientSettings {
        self.workspace_for_url(url).map_or_else(
            || {
                tracing::warn!(
                    "Workspace not found for {url}. Global settings will be used for this document"
                );
                settings::ResolvedClientSettings::global(global_settings)
            },
            |workspace| {
                settings::ResolvedClientSettings::with_workspace(
                    &workspace.settings,
                    global_settings,
                )
            },
        )
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
    pub(crate) fn new(root: &Url, settings: ClientSettings) -> crate::Result<(PathBuf, Self)> {
        let path = root
            .to_file_path()
            .map_err(|()| anyhow!("workspace URL was not a file path!"))?;

        Ok((
            path,
            Self {
                open_documents: OpenDocuments::default(),
                settings,
            },
        ))
    }

    fn reload_configuration(&mut self) {
        self.open_documents.reload_configuration();
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
        let configuration = self.configuration_index.get_or_insert(url);
        if self
            .documents
            .insert(
                url.clone(),
                DocumentController::new(contents, version, configuration),
            )
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

    fn reload_configuration(&mut self) {
        self.configuration_index.clear();

        for (path, document) in &mut self.documents {
            let new_configuration = self.configuration_index.get_or_insert(path);
            document.update_configuration(new_configuration);
        }
    }
}

impl DocumentController {
    fn new(
        contents: String,
        version: DocumentVersion,
        configuration: Arc<RuffConfiguration>,
    ) -> Self {
        Self {
            document: Arc::new(Document::new(contents, version)),
            configuration,
        }
    }

    pub(crate) fn update_configuration(&mut self, new_configuration: Arc<RuffConfiguration>) {
        self.configuration = new_configuration;
    }

    pub(crate) fn make_ref(&self) -> DocumentRef {
        DocumentRef {
            document: self.document.clone(),
            configuration: self.configuration.clone(),
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

impl DocumentRef {
    pub(crate) fn configuration(&self) -> &RuffConfiguration {
        &self.configuration
    }
}
