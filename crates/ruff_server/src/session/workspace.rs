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

use self::ruff_settings::RuffSettingsIndex;

use super::{
    settings::{self, ResolvedClientSettings, ResolvedEditorSettings},
    ClientSettings,
};

mod ruff_settings;

pub(crate) use ruff_settings::RuffSettings;

#[derive(Default)]
pub(crate) struct Workspaces(BTreeMap<PathBuf, Workspace>);

pub(crate) struct Workspace {
    open_documents: OpenDocuments,
    settings: ResolvedClientSettings,
}

pub(crate) struct OpenDocuments {
    documents: FxHashMap<Url, DocumentController>,
    settings_index: ruff_settings::RuffSettingsIndex,
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
    settings: Arc<RuffSettings>,
}

impl Workspaces {
    pub(super) fn new(
        workspaces: Vec<(Url, ClientSettings)>,
        global_settings: &ClientSettings,
    ) -> crate::Result<Self> {
        Ok(Self(
            workspaces
                .into_iter()
                .map(|(url, workspace_settings)| {
                    Workspace::new(&url, &workspace_settings, global_settings)
                })
                .collect::<crate::Result<_>>()?,
        ))
    }

    pub(super) fn open_workspace_folder(
        &mut self,
        folder_url: &Url,
        global_settings: &ClientSettings,
    ) -> crate::Result<()> {
        // TODO(jane): find a way to allow for workspace settings to be updated dynamically
        let (path, workspace) =
            Workspace::new(folder_url, &ClientSettings::default(), global_settings)?;
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

    pub(super) fn reload_settings(&mut self, changed_url: &Url) -> crate::Result<()> {
        let (root, workspace) = self
            .entry_for_url_mut(changed_url)
            .ok_or_else(|| anyhow!("Workspace not found for {changed_url}"))?;
        workspace.reload_settings(root);
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
            |workspace| workspace.settings.clone(),
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
    pub(crate) fn new(
        root: &Url,
        workspace_settings: &ClientSettings,
        global_settings: &ClientSettings,
    ) -> crate::Result<(PathBuf, Self)> {
        let path = root
            .to_file_path()
            .map_err(|()| anyhow!("workspace URL was not a file path!"))?;

        let settings = ResolvedClientSettings::with_workspace(workspace_settings, global_settings);

        let workspace = Self {
            open_documents: OpenDocuments::new(&path, settings.editor_settings()),
            settings,
        };

        Ok((path, workspace))
    }

    fn reload_settings(&mut self, root: &Path) {
        self.open_documents
            .reload_settings(root, self.settings.editor_settings());
    }
}

impl OpenDocuments {
    fn new(path: &Path, editor_settings: &ResolvedEditorSettings) -> Self {
        Self {
            documents: FxHashMap::default(),
            settings_index: RuffSettingsIndex::new(path, editor_settings),
        }
    }

    fn snapshot(&self, url: &Url) -> Option<DocumentRef> {
        let path = url
            .to_file_path()
            .expect("document URL should convert to file path: {url}");
        let document_settings = self.settings_index.get(&path);
        Some(self.documents.get(url)?.make_ref(document_settings))
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

    fn reload_settings(&mut self, root: &Path, editor_settings: &ResolvedEditorSettings) {
        self.settings_index = RuffSettingsIndex::new(root, editor_settings);
    }
}

impl DocumentController {
    fn new(contents: String, version: DocumentVersion) -> Self {
        Self {
            document: Arc::new(Document::new(contents, version)),
        }
    }

    pub(crate) fn make_ref(&self, document_settings: Arc<RuffSettings>) -> DocumentRef {
        DocumentRef {
            document: self.document.clone(),
            settings: document_settings,
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
    pub(crate) fn settings(&self) -> &RuffSettings {
        &self.settings
    }
}
