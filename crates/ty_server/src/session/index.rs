use std::path::Path;
use std::sync::Arc;

use lsp_types::Url;
use rustc_hash::FxHashMap;

use crate::{
    document::{DocumentKey, DocumentVersion, NotebookDocument},
    PositionEncoding, TextDocument,
};

use super::ClientSettings;

/// Stores and tracks all open documents in a session, along with their associated settings.
#[derive(Default, Debug)]
pub(crate) struct Index {
    /// Maps all document file URLs to the associated document controller
    documents: FxHashMap<Url, DocumentController>,

    /// Maps opaque cell URLs to a notebook URL (document)
    notebook_cells: FxHashMap<Url, Url>,

    /// Global settings provided by the client.
    #[expect(dead_code)]
    global_settings: ClientSettings,
}

impl Index {
    pub(super) fn new(global_settings: ClientSettings) -> Self {
        Self {
            documents: FxHashMap::default(),
            notebook_cells: FxHashMap::default(),
            global_settings,
        }
    }

    #[expect(dead_code)]
    pub(super) fn text_document_urls(&self) -> impl Iterator<Item = &Url> + '_ {
        self.documents
            .iter()
            .filter_map(|(url, doc)| doc.as_text().and(Some(url)))
    }

    #[expect(dead_code)]
    pub(super) fn notebook_document_urls(&self) -> impl Iterator<Item = &Url> + '_ {
        self.documents
            .iter()
            .filter(|(_, doc)| doc.as_notebook().is_some())
            .map(|(url, _)| url)
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

    pub(crate) fn key_from_url(&self, url: Url) -> DocumentKey {
        if self.notebook_cells.contains_key(&url) {
            DocumentKey::NotebookCell(url)
        } else if Path::new(url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ipynb"))
        {
            DocumentKey::Notebook(url)
        } else {
            DocumentKey::Text(url)
        }
    }

    #[expect(dead_code)]
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
            did_open: Some(did_open),
            ..
        }) = cells.as_ref().and_then(|cells| cells.structure.as_ref())
        {
            let Some(path) = self.url_for_key(key).cloned() else {
                anyhow::bail!("Tried to open unavailable document `{key}`");
            };

            for opened_cell in did_open {
                self.notebook_cells
                    .insert(opened_cell.uri.clone(), path.clone());
            }
            // deleted notebook cells are closed via textDocument/didClose - we don't close them here.
        }

        let controller = self.document_controller_for_key(key)?;
        let Some(notebook) = controller.as_notebook_mut() else {
            anyhow::bail!("Notebook document URI does not point to a notebook document");
        };

        notebook.update(cells, metadata, new_version, encoding)?;
        Ok(())
    }

    pub(crate) fn make_document_ref(&self, key: DocumentKey) -> Option<DocumentQuery> {
        let url = self.url_for_key(&key)?.clone();
        let controller = self.documents.get(&url)?;
        let cell_url = match key {
            DocumentKey::NotebookCell(cell_url) => Some(cell_url),
            _ => None,
        };
        Some(controller.make_ref(cell_url, url))
    }

    pub(super) fn open_text_document(&mut self, url: Url, document: TextDocument) {
        self.documents
            .insert(url, DocumentController::new_text(document));
    }

    pub(super) fn open_notebook_document(&mut self, notebook_url: Url, document: NotebookDocument) {
        for cell_url in document.urls() {
            self.notebook_cells
                .insert(cell_url.clone(), notebook_url.clone());
        }
        self.documents
            .insert(notebook_url, DocumentController::new_notebook(document));
    }

    pub(super) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        // Notebook cells URIs are removed from the index here, instead of during
        // `update_notebook_document`. This is because a notebook cell, as a text document,
        // is requested to be `closed` by VS Code after the notebook gets updated.
        // This is not documented in the LSP specification explicitly, and this assumption
        // may need revisiting in the future as we support more editors with notebook support.
        if let DocumentKey::NotebookCell(uri) = key {
            if self.notebook_cells.remove(uri).is_none() {
                tracing::warn!("Tried to remove a notebook cell that does not exist: {uri}",);
            }
            return Ok(());
        }
        let Some(url) = self.url_for_key(key).cloned() else {
            anyhow::bail!("Tried to close unavailable document `{key}`");
        };

        let Some(_) = self.documents.remove(&url) else {
            anyhow::bail!("tried to close document that didn't exist at {}", url)
        };
        Ok(())
    }

    fn document_controller_for_key(
        &mut self,
        key: &DocumentKey,
    ) -> crate::Result<&mut DocumentController> {
        let Some(url) = self.url_for_key(key).cloned() else {
            anyhow::bail!("Tried to open unavailable document `{key}`");
        };
        let Some(controller) = self.documents.get_mut(&url) else {
            anyhow::bail!("Document controller not available at `{}`", url);
        };
        Ok(controller)
    }

    fn url_for_key<'a>(&'a self, key: &'a DocumentKey) -> Option<&'a Url> {
        match key {
            DocumentKey::Notebook(path) | DocumentKey::Text(path) => Some(path),
            DocumentKey::NotebookCell(uri) => self.notebook_cells.get(uri),
        }
    }
}

/// A mutable handler to an underlying document.
#[derive(Debug)]
enum DocumentController {
    Text(Arc<TextDocument>),
    Notebook(Arc<NotebookDocument>),
}

impl DocumentController {
    fn new_text(document: TextDocument) -> Self {
        Self::Text(Arc::new(document))
    }

    fn new_notebook(document: NotebookDocument) -> Self {
        Self::Notebook(Arc::new(document))
    }

    fn make_ref(&self, cell_url: Option<Url>, file_url: Url) -> DocumentQuery {
        match &self {
            Self::Notebook(notebook) => DocumentQuery::Notebook {
                cell_url,
                file_url,
                notebook: notebook.clone(),
            },
            Self::Text(document) => DocumentQuery::Text {
                file_url,
                document: document.clone(),
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

/// A read-only query to an open document.
/// This query can 'select' a text document, full notebook, or a specific notebook cell.
/// It also includes document settings.
#[derive(Debug, Clone)]
pub enum DocumentQuery {
    Text {
        file_url: Url,
        document: Arc<TextDocument>,
    },
    Notebook {
        /// The selected notebook cell, if it exists.
        cell_url: Option<Url>,
        /// The URL of the notebook.
        file_url: Url,
        notebook: Arc<NotebookDocument>,
    },
}

impl DocumentQuery {
    /// Retrieve the original key that describes this document query.
    #[expect(dead_code)]
    pub(crate) fn make_key(&self) -> DocumentKey {
        match self {
            Self::Text { file_url, .. } => DocumentKey::Text(file_url.clone()),
            Self::Notebook {
                cell_url: Some(cell_uri),
                ..
            } => DocumentKey::NotebookCell(cell_uri.clone()),
            Self::Notebook { file_url, .. } => DocumentKey::Notebook(file_url.clone()),
        }
    }

    /// Attempts to access the underlying notebook document that this query is selecting.
    pub fn as_notebook(&self) -> Option<&NotebookDocument> {
        match self {
            Self::Notebook { notebook, .. } => Some(notebook),
            Self::Text { .. } => None,
        }
    }

    /// Get the version of document selected by this query.
    pub(crate) fn version(&self) -> DocumentVersion {
        match self {
            Self::Text { document, .. } => document.version(),
            Self::Notebook { notebook, .. } => notebook.version(),
        }
    }

    /// Get the URL for the document selected by this query.
    pub(crate) fn file_url(&self) -> &Url {
        match self {
            Self::Text { file_url, .. } | Self::Notebook { file_url, .. } => file_url,
        }
    }

    /// Attempt to access the single inner text document selected by the query.
    /// If this query is selecting an entire notebook document, this will return `None`.
    #[expect(dead_code)]
    pub(crate) fn as_single_document(&self) -> Option<&TextDocument> {
        match self {
            Self::Text { document, .. } => Some(document),
            Self::Notebook {
                notebook,
                cell_url: cell_uri,
                ..
            } => cell_uri
                .as_ref()
                .and_then(|cell_uri| notebook.cell_document_by_uri(cell_uri)),
        }
    }
}
