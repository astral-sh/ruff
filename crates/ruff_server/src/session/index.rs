use anyhow::anyhow;
use rustc_hash::FxHashMap;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    edit::{DocumentKey, DocumentVersion, NotebookDocument},
    PositionEncoding, TextDocument,
};

use super::{
    settings::{self, ResolvedClientSettings},
    ClientSettings,
};

mod ruff_settings;

pub(crate) use ruff_settings::RuffSettings;

type DocumentIndex = FxHashMap<PathBuf, DocumentController>;
type NotebookCellIndex = FxHashMap<lsp_types::Url, PathBuf>;
type SettingsIndex = BTreeMap<PathBuf, WorkspaceSettings>;

/// Stores and tracks all open documents in a session, along with their associated settings.
#[derive(Default)]
pub(crate) struct Index {
    /// Maps all document file paths to the associated document controller
    documents: DocumentIndex,
    /// Maps opaque cell URLs to a notebook path
    notebook_cells: NotebookCellIndex,
    /// Maps a workspace folder root to its settings.
    settings: SettingsIndex,
}

/// Settings associated with a workspace.
struct WorkspaceSettings {
    client_settings: ResolvedClientSettings,
    workspace_settings_index: ruff_settings::RuffSettingsIndex,
}

/// A mutable handler to an underlying document.
enum DocumentController {
    Text(Arc<TextDocument>),
    Notebook(Arc<NotebookDocument>),
}

/// A read-only query to an open document.
/// This query can 'select' a text document, full notebook, or a specific notebook cell.
/// It also includes document settings.
#[derive(Clone)]
pub(crate) enum DocumentQuery {
    Text {
        file_path: PathBuf,
        document: Arc<TextDocument>,
        settings: Arc<RuffSettings>,
    },
    Notebook {
        /// The selected notebook cell, if it exists.
        cell_uri: Option<lsp_types::Url>,
        file_path: PathBuf,
        notebook: Arc<NotebookDocument>,
        settings: Arc<RuffSettings>,
    },
}

impl Index {
    pub(super) fn new(
        workspace_folders: Vec<(PathBuf, ClientSettings)>,
        global_settings: &ClientSettings,
    ) -> Self {
        let mut settings_index = BTreeMap::new();
        for (path, workspace_settings) in workspace_folders {
            Self::register_workspace_settings(
                &mut settings_index,
                path,
                Some(workspace_settings),
                global_settings,
            );
        }

        Self {
            documents: FxHashMap::default(),
            notebook_cells: FxHashMap::default(),
            settings: settings_index,
        }
    }

    pub(super) fn update_text_document(
        &mut self,
        key: &DocumentKey,
        content_changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> crate::Result<()> {
        let controller = self.document_controller_for_key(key)?;
        let Some(document) = controller.as_text_mut() else {
            anyhow::bail!("Text document URI does not point to a text document");
        };

        if content_changes.is_empty() {
            document.update_version(new_version);
            return Ok(());
        }

        document.apply_changes(content_changes, new_version, encoding);

        Ok(())
    }

    pub(super) fn key_from_url(&self, url: &lsp_types::Url) -> Option<DocumentKey> {
        if self.notebook_cells.contains_key(url) {
            return Some(DocumentKey::NotebookCell(url.clone()));
        }
        let path = url.to_file_path().ok()?;
        Some(
            match path
                .extension()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default()
            {
                "ipynb" => DocumentKey::Notebook(path),
                _ => DocumentKey::Text(path),
            },
        )
    }

    pub(super) fn update_notebook_document(
        &mut self,
        key: &DocumentKey,
        cells: Option<lsp_types::NotebookDocumentCellChange>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> crate::Result<()> {
        // update notebook cell index
        if let Some(lsp_types::NotebookDocumentCellChangeStructure {
            did_open,
            did_close,
            ..
        }) = cells.as_ref().and_then(|cells| cells.structure.as_ref())
        {
            let Some(path) = self.path_for_key(key).cloned() else {
                anyhow::bail!("Tried to open unavailable document `{key}`");
            };

            for opened_cell in did_open.iter().flatten() {
                self.notebook_cells
                    .insert(opened_cell.uri.clone(), path.clone());
            }
            for closed_cell in did_close.iter().flatten() {
                if self.notebook_cells.remove(&closed_cell.uri).is_none() {
                    tracing::warn!(
                        "Tried to remove a notebook cell that does not exist: {}",
                        closed_cell.uri
                    );
                }
            }
        }

        let controller = self.document_controller_for_key(key)?;
        let Some(notebook) = controller.as_notebook_mut() else {
            anyhow::bail!("Notebook document URI does not point to a notebook document");
        };

        notebook.update(cells, metadata, new_version, encoding)?;
        Ok(())
    }

    pub(super) fn open_workspace_folder(
        &mut self,
        path: PathBuf,
        global_settings: &ClientSettings,
    ) {
        // TODO(jane): Find a way for workspace client settings to be added or changed dynamically.
        Self::register_workspace_settings(&mut self.settings, path, None, global_settings);
    }

    fn register_workspace_settings(
        settings_index: &mut SettingsIndex,
        workspace_path: PathBuf,
        workspace_settings: Option<ClientSettings>,
        global_settings: &ClientSettings,
    ) {
        let client_settings = if let Some(workspace_settings) = workspace_settings {
            ResolvedClientSettings::with_workspace(&workspace_settings, global_settings)
        } else {
            ResolvedClientSettings::global(global_settings)
        };
        let workspace_settings_index = ruff_settings::RuffSettingsIndex::new(
            &workspace_path,
            client_settings.editor_settings(),
        );

        settings_index.insert(
            workspace_path,
            WorkspaceSettings {
                client_settings,
                workspace_settings_index,
            },
        );
    }

    pub(super) fn close_workspace_folder(&mut self, workspace_path: &PathBuf) -> crate::Result<()> {
        self.settings.remove(workspace_path).ok_or_else(|| {
            anyhow!(
                "Tried to remove non-existent folder {}",
                workspace_path.display()
            )
        })?;
        // O(n) complexity, which isn't ideal... but this is an uncommon operation.
        self.documents
            .retain(|path, _| !path.starts_with(workspace_path));
        self.notebook_cells
            .retain(|_, path| !path.starts_with(workspace_path));
        Ok(())
    }

    pub(super) fn make_document_ref(&self, key: DocumentKey) -> Option<DocumentQuery> {
        let path = self.path_for_key(&key)?.clone();
        let document_settings = self
            .settings_for_path(&path)?
            .workspace_settings_index
            .get(&path);

        let controller = self.documents.get(&path)?;
        let cell_uri = match key {
            DocumentKey::NotebookCell(uri) => Some(uri),
            _ => None,
        };
        Some(controller.make_ref(cell_uri, path, document_settings))
    }

    pub(super) fn reload_settings(&mut self, changed_path: &PathBuf) {
        for (root, settings) in self
            .settings
            .iter_mut()
            .filter(|(path, _)| path.starts_with(changed_path))
        {
            settings.workspace_settings_index = ruff_settings::RuffSettingsIndex::new(
                root,
                settings.client_settings.editor_settings(),
            );
        }
    }

    pub(super) fn open_text_document(&mut self, path: PathBuf, document: TextDocument) {
        self.documents
            .insert(path, DocumentController::new_text(document));
    }

    pub(super) fn open_notebook_document(&mut self, path: PathBuf, document: NotebookDocument) {
        for url in document.urls() {
            self.notebook_cells.insert(url.clone(), path.clone());
        }
        self.documents
            .insert(path, DocumentController::new_notebook(document));
    }

    pub(super) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        let Some(path) = self.path_for_key(key).cloned() else {
            anyhow::bail!("Tried to open unavailable document `{key}`");
        };

        let Some(controller) = self.documents.remove(&path) else {
            anyhow::bail!(
                "tried to close document that didn't exist at {}",
                path.display()
            )
        };
        if let Some(notebook) = controller.as_notebook() {
            for url in notebook.urls() {
                self.notebook_cells.remove(url).ok_or_else(|| {
                    anyhow!("tried to de-register notebook cell with URL {url} that didn't exist")
                })?;
            }
        }
        Ok(())
    }

    pub(super) fn client_settings(
        &self,
        key: &DocumentKey,
        global_settings: &ClientSettings,
    ) -> settings::ResolvedClientSettings {
        let Some(path) = self.path_for_key(key) else {
            return ResolvedClientSettings::global(global_settings);
        };
        let Some(WorkspaceSettings {
            client_settings, ..
        }) = self.settings_for_path(path)
        else {
            return ResolvedClientSettings::global(global_settings);
        };
        client_settings.clone()
    }

    fn document_controller_for_key(
        &mut self,
        key: &DocumentKey,
    ) -> crate::Result<&mut DocumentController> {
        let Some(path) = self.path_for_key(key).cloned() else {
            anyhow::bail!("Tried to open unavailable document `{key}`");
        };
        let Some(controller) = self.documents.get_mut(&path) else {
            anyhow::bail!("Document controller not available at `{}`", path.display());
        };
        Ok(controller)
    }

    fn path_for_key<'a>(&'a self, key: &'a DocumentKey) -> Option<&'a PathBuf> {
        match key {
            DocumentKey::Notebook(path) | DocumentKey::Text(path) => Some(path),
            DocumentKey::NotebookCell(uri) => self.notebook_cells.get(uri),
        }
    }

    fn settings_for_path(&self, path: &Path) -> Option<&WorkspaceSettings> {
        self.settings
            .range(..path.to_path_buf())
            .next_back()
            .map(|(_, settings)| settings)
    }
}

impl DocumentController {
    fn new_text(document: TextDocument) -> Self {
        Self::Text(Arc::new(document))
    }

    fn new_notebook(document: NotebookDocument) -> Self {
        Self::Notebook(Arc::new(document))
    }

    fn make_ref(
        &self,
        cell_uri: Option<lsp_types::Url>,
        file_path: PathBuf,
        settings: Arc<RuffSettings>,
    ) -> DocumentQuery {
        match &self {
            Self::Notebook(notebook) => DocumentQuery::Notebook {
                cell_uri,
                file_path,
                notebook: notebook.clone(),
                settings,
            },
            Self::Text(document) => DocumentQuery::Text {
                file_path,
                document: document.clone(),
                settings,
            },
        }
    }

    pub(crate) fn as_notebook_mut(&mut self) -> Option<&mut NotebookDocument> {
        Some(match self {
            Self::Notebook(notebook) => Arc::make_mut(notebook),
            Self::Text(_) => return None,
        })
    }

    pub(crate) fn as_notebook(&self) -> Option<&NotebookDocument> {
        match self {
            Self::Notebook(notebook) => Some(notebook),
            Self::Text(_) => None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn as_text(&self) -> Option<&TextDocument> {
        match self {
            Self::Text(document) => Some(document),
            Self::Notebook(_) => None,
        }
    }

    pub(crate) fn as_text_mut(&mut self) -> Option<&mut TextDocument> {
        Some(match self {
            Self::Text(document) => Arc::make_mut(document),
            Self::Notebook(_) => return None,
        })
    }
}

impl DocumentQuery {
    /// Retrieve the original key that describes this document query.
    pub(crate) fn make_key(&self) -> DocumentKey {
        match self {
            Self::Text { file_path, .. } => DocumentKey::Text(file_path.clone()),
            Self::Notebook {
                cell_uri: Some(cell_uri),
                ..
            } => DocumentKey::NotebookCell(cell_uri.clone()),
            Self::Notebook { file_path, .. } => DocumentKey::Notebook(file_path.clone()),
        }
    }

    /// Get the document settings associated with this query.
    pub(crate) fn settings(&self) -> &RuffSettings {
        match self {
            Self::Text { settings, .. } | Self::Notebook { settings, .. } => settings,
        }
    }

    /// Generate a source kind used by the linter.
    pub(crate) fn make_source_kind(&self) -> ruff_linter::source_kind::SourceKind {
        match self {
            Self::Text { document, .. } => {
                ruff_linter::source_kind::SourceKind::Python(document.contents().to_string())
            }
            Self::Notebook { notebook, .. } => {
                ruff_linter::source_kind::SourceKind::IpyNotebook(notebook.make_ruff_notebook())
            }
        }
    }

    /// Attempts to access the underlying notebook document that this query is selecting.
    pub(crate) fn as_notebook(&self) -> Option<&NotebookDocument> {
        match self {
            Self::Notebook { notebook, .. } => Some(notebook),
            Self::Text { .. } => None,
        }
    }

    /// Get the source type of the document associated with this query.
    pub(crate) fn source_type(&self) -> ruff_python_ast::PySourceType {
        match self {
            Self::Text { .. } => ruff_python_ast::PySourceType::Python,
            Self::Notebook { .. } => ruff_python_ast::PySourceType::Ipynb,
        }
    }

    /// Get the version of document selected by this query.
    pub(crate) fn version(&self) -> DocumentVersion {
        match self {
            Self::Text { document, .. } => document.version(),
            Self::Notebook { notebook, .. } => notebook.version(),
        }
    }

    /// Get the underlying file path for the document selected by this query.
    pub(crate) fn file_path(&self) -> &PathBuf {
        match self {
            Self::Text { file_path, .. } | Self::Notebook { file_path, .. } => file_path,
        }
    }

    /// Attempt to access the single inner text document selected by the query.
    /// If this query is selecting an entire notebook document, this will return `None`.
    pub(crate) fn as_single_document(&self) -> Option<&TextDocument> {
        match self {
            Self::Text { document, .. } => Some(document),
            Self::Notebook {
                notebook, cell_uri, ..
            } => cell_uri
                .as_ref()
                .and_then(|cell_uri| notebook.cell_document_by_uri(cell_uri)),
        }
    }
}
