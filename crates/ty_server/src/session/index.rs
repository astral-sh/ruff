use std::sync::Arc;

use crate::{
    PositionEncoding, TextDocument,
    document::{DocumentKey, DocumentVersion, NotebookDocument},
    system::AnySystemPath,
};
use lsp_types::Url;
use ruff_db::Db;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{SystemVirtualPath, SystemVirtualPathBuf};
use rustc_hash::FxHashMap;

/// Stores and tracks all open documents in a session, along with their associated settings.
#[derive(Debug)]
pub(crate) struct Index {
    /// Maps all document file paths to the associated document controller
    documents: FxHashMap<AnySystemPath, Document>,

    /// Maps opaque cell URLs to a notebook path (document)
    notebook_cells: FxHashMap<SystemVirtualPathBuf, AnySystemPath>,
}

impl Index {
    pub(super) fn new() -> Self {
        Self {
            documents: FxHashMap::default(),
            notebook_cells: FxHashMap::default(),
        }
    }

    pub(super) fn text_document_urls(&self) -> impl Iterator<Item = &Url> + '_ {
        self.documents.values().filter_map(|doc| {
            let text_document = doc.as_text()?;
            Some(text_document.url())
        })
    }

    #[expect(dead_code)]
    pub(super) fn notebook_document_paths(&self) -> impl Iterator<Item = &AnySystemPath> + '_ {
        self.documents
            .iter()
            .filter(|(_, doc)| doc.as_notebook().is_some())
            .map(|(path, _)| path)
    }

    pub(super) fn update_text_document(
        &mut self,
        path: &AnySystemPath,
        content_changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> crate::Result<()> {
        let controller = self.document_mut(path)?;
        let Some(document) = controller.as_text_mut() else {
            anyhow::bail!("Text document path does not point to a text document");
        };

        if content_changes.is_empty() {
            document.update_version(new_version);
            return Ok(());
        }

        document.apply_changes(content_changes, new_version, encoding);

        Ok(())
    }

    /// Returns the [`DocumentKey`] corresponding to the given URL.
    pub(crate) fn key_from_url(&self, url: Url) -> DocumentKey {
        let path = AnySystemPath::from_url(&url);

        if let Some(notebook_path) = path
            .as_virtual()
            .and_then(|path| self.notebook_cells.get(&path.to_path_buf()))
        {
            DocumentKey::NotebookCell {
                cell_url: url,
                notebook_path: notebook_path.clone(),
            }
        } else {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ipynb"))
            {
                DocumentKey::Notebook { url }
            } else {
                DocumentKey::Text { url }
            }
        }
    }

    #[expect(dead_code)]
    pub(super) fn update_notebook_document(
        &mut self,
        path: &AnySystemPath,
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
            for opened_cell in did_open {
                let cell_path = SystemVirtualPath::new(opened_cell.uri.as_str());
                self.notebook_cells
                    .insert(cell_path.to_path_buf(), path.clone());
            }
            // deleted notebook cells are closed via textDocument/didClose - we don't close them here.
        }

        let controller = self.document_mut(path)?;
        let Some(notebook) = controller.as_notebook_mut() else {
            anyhow::bail!("Notebook document path does not point to a notebook document");
        };

        notebook.update(cells, metadata, new_version, encoding)?;
        Ok(())
    }

    /// Create a document reference corresponding to the given document key.
    ///
    /// Returns an error if the document is not found or if the path cannot be converted to a URL.
    pub(crate) fn make_document_ref(
        &self,
        path: &AnySystemPath,
    ) -> Result<DocumentRef, DocumentError> {
        let Some(controller) = self.documents.get(&path) else {
            return Err(DocumentError::NotFound(path.clone()));
        };

        if let Some(r#virtual) = path.as_virtual() {
            let cell_path = r#virtual.to_path_buf();
            if let Some(notebook_path) = self.notebook_cells.get(&cell_path) {
                return Ok(controller.make_ref(Some(cell_path), notebook_path.clone()));
            }
        }

        Ok(controller.make_ref(None, path.clone()))
    }

    pub(super) fn open_text_document(&mut self, path: &AnySystemPath, document: TextDocument) {
        self.documents
            .insert(path.clone(), Document::new_text(document));
    }

    pub(super) fn open_notebook_document(
        &mut self,
        notebook_path: &AnySystemPath,
        document: NotebookDocument,
    ) {
        for cell_url in document.cell_urls() {
            let cell_path = SystemVirtualPath::new(cell_url.as_str());
            self.notebook_cells
                .insert(cell_path.to_path_buf(), notebook_path.clone());
        }
        self.documents
            .insert(notebook_path.clone(), Document::new_notebook(document));
    }

    pub(super) fn close_document(&mut self, path: &AnySystemPath) -> crate::Result<()> {
        // Notebook cells URIs are removed from the index here, instead of during
        // `update_notebook_document`. This is because a notebook cell, as a text document,
        // is requested to be `closed` by VS Code after the notebook gets updated.
        // This is not documented in the LSP specification explicitly, and this assumption
        // may need revisiting in the future as we support more editors with notebook support.
        if let AnySystemPath::SystemVirtual(r#virtual) = path {
            self.notebook_cells.remove(r#virtual);
        }

        let Some(_) = self.documents.remove(&path) else {
            anyhow::bail!("tried to close document that didn't exist at {path}")
        };
        Ok(())
    }

    fn document_mut(&mut self, path: &AnySystemPath) -> Result<&mut Document, DocumentError> {
        let Some(controller) = self.documents.get_mut(&path) else {
            return Err(DocumentError::NotFound(path.clone()));
        };
        Ok(controller)
    }
}

/// A mutable handler to an underlying document.
#[derive(Debug)]
enum Document {
    Text(Arc<TextDocument>),
    Notebook(Arc<NotebookDocument>),
}

impl Document {
    fn new_text(document: TextDocument) -> Self {
        Self::Text(Arc::new(document))
    }

    fn new_notebook(document: NotebookDocument) -> Self {
        Self::Notebook(Arc::new(document))
    }

    fn make_ref(
        &self,
        cell_path: Option<SystemVirtualPathBuf>,
        file_path: AnySystemPath,
    ) -> DocumentRef {
        match &self {
            Self::Notebook(notebook) => DocumentRef::Notebook {
                cell_path,
                file_path,
                notebook: notebook.clone(),
            },
            Self::Text(document) => DocumentRef::Text {
                file_path,
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
///
/// This query can 'select' a text document, full notebook, or a specific notebook cell.
/// It also includes document settings.
#[derive(Debug, Clone)]
pub(crate) enum DocumentRef {
    Text {
        file_path: AnySystemPath,
        document: Arc<TextDocument>,
    },
    Notebook {
        /// The selected notebook cell, if it exists.
        cell_path: Option<SystemVirtualPathBuf>,
        /// The path to the notebook.
        file_path: AnySystemPath,
        notebook: Arc<NotebookDocument>,
    },
}

impl DocumentRef {
    /// Attempts to access the underlying notebook document that this query is selecting.
    pub(crate) fn as_notebook(&self) -> Option<&NotebookDocument> {
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

    /// Get the system path for the document selected by this query.
    pub(crate) fn file_path(&self) -> &AnySystemPath {
        match self {
            Self::Text { file_path, .. } | Self::Notebook { file_path, .. } => file_path,
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
                cell_path: cell_uri,
                ..
            } => cell_uri
                .as_ref()
                .and_then(|cell_uri| notebook.cell_document_by_uri(cell_uri)),
        }
    }

    /// Returns the salsa interned [`File`] for the document selected by this query.
    ///
    /// It returns [`None`] for the following cases:
    /// - For virtual file, if it's not yet opened
    /// - For regular file, if it does not exists or is a directory
    pub(crate) fn file(&self, db: &dyn Db) -> Option<File> {
        match self.file_path() {
            AnySystemPath::System(path) => system_path_to_file(db, path).ok(),
            AnySystemPath::SystemVirtual(virtual_path) => db
                .files()
                .try_virtual_file(virtual_path)
                .map(|virtual_file| virtual_file.file()),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum DocumentError {
    #[error("document not found for path: {0}")]
    NotFound(AnySystemPath),
}
