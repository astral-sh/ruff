use std::sync::Arc;

use crate::document::DocumentKey;
use crate::session::DocumentHandle;
use crate::{
    PositionEncoding, TextDocument,
    document::{DocumentVersion, NotebookDocument},
    system::AnySystemPath,
};

use ruff_db::system::SystemVirtualPath;
use rustc_hash::FxHashMap;

/// Stores and tracks all open documents in a session, along with their associated settings.
#[derive(Debug)]
pub(crate) struct Index {
    /// Maps all document file paths to the associated document controller
    documents: FxHashMap<DocumentKey, Document>,

    /// Maps opaque cell URLs to a notebook path (document)
    notebook_cells: FxHashMap<String, AnySystemPath>,
}

impl Index {
    pub(super) fn new() -> Self {
        Self {
            documents: FxHashMap::default(),
            notebook_cells: FxHashMap::default(),
        }
    }

    pub(super) fn text_documents(
        &self,
    ) -> impl Iterator<Item = (&DocumentKey, &TextDocument)> + '_ {
        self.documents.iter().filter_map(|(key, doc)| {
            let text_document = doc.as_text()?;
            Some((key, text_document))
        })
    }

    pub(crate) fn document_handle(
        &self,
        url: &lsp_types::Url,
    ) -> Result<DocumentHandle, DocumentError> {
        let key = DocumentKey::from_url(url);
        let Some(document) = self.documents.get(&key) else {
            return Err(DocumentError::NotFound(key));
        };

        if let Some(path) = key.as_opaque() {
            if let Some(notebook_path) = self.notebook_cells.get(path) {
                return Ok(DocumentHandle {
                    key: key.clone(),
                    notebook_path: Some(notebook_path.clone()),
                    url: url.clone(),
                    version: document.version(),
                });
            }
        }

        Ok(DocumentHandle {
            key: key.clone(),
            notebook_path: None,
            url: url.clone(),
            version: document.version(),
        })
    }

    #[expect(dead_code)]
    pub(super) fn notebook_document_keys(&self) -> impl Iterator<Item = &DocumentKey> + '_ {
        self.documents
            .iter()
            .filter(|(_, doc)| doc.as_notebook().is_some())
            .map(|(key, _)| key)
    }

    #[expect(dead_code)]
    pub(super) fn update_notebook_document(
        &mut self,
        notebook_key: &DocumentKey,
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
                    .insert(cell_path.to_string(), notebook_key.to_file_path());
            }
            // deleted notebook cells are closed via textDocument/didClose - we don't close them here.
        }

        let document = self.document_mut(notebook_key)?;
        let Some(notebook) = document.as_notebook_mut() else {
            anyhow::bail!("Notebook document path does not point to a notebook document");
        };

        notebook.update(cells, metadata, new_version, encoding)?;
        Ok(())
    }

    /// Create a document reference corresponding to the given document key.
    ///
    /// Returns an error if the document is not found or if the path cannot be converted to a URL.
    pub(crate) fn document(&self, key: &DocumentKey) -> Result<&Document, DocumentError> {
        let Some(document) = self.documents.get(key) else {
            return Err(DocumentError::NotFound(key.clone()));
        };

        Ok(document)
    }

    pub(crate) fn notebook_arc(
        &self,
        key: &DocumentKey,
    ) -> Result<Arc<NotebookDocument>, DocumentError> {
        let Some(document) = self.documents.get(key) else {
            return Err(DocumentError::NotFound(key.clone()));
        };

        if let Document::Notebook(notebook) = document {
            Ok(notebook.clone())
        } else {
            Err(DocumentError::NotFound(key.clone()))
        }
    }

    pub(super) fn open_text_document(&mut self, document: TextDocument) -> DocumentHandle {
        let key = DocumentKey::from_url(document.url());

        // TODO: Fix file path for notebook cells
        let handle = DocumentHandle {
            key: key.clone(),
            notebook_path: None,
            url: document.url().clone(),
            version: document.version(),
        };

        self.documents.insert(key, Document::new_text(document));

        handle
    }

    pub(super) fn open_notebook_document(&mut self, document: NotebookDocument) -> DocumentHandle {
        let notebook_key = DocumentKey::from_url(document.url());
        let url = document.url().clone();
        let version = document.version();

        for cell_url in document.cell_urls() {
            self.notebook_cells
                .insert(cell_url.to_string(), notebook_key.to_file_path());
        }

        self.documents
            .insert(notebook_key.clone(), Document::new_notebook(document));

        DocumentHandle {
            notebook_path: Some(notebook_key.to_file_path()),
            key: notebook_key,
            url,
            version,
        }
    }

    pub(super) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        // Notebook cells URIs are removed from the index here, instead of during
        // `update_notebook_document`. This is because a notebook cell, as a text document,
        // is requested to be `closed` by VS Code after the notebook gets updated.
        // This is not documented in the LSP specification explicitly, and this assumption
        // may need revisiting in the future as we support more editors with notebook support.
        if let DocumentKey::Opaque(uri) = key {
            self.notebook_cells.remove(uri);
        }

        let Some(_) = self.documents.remove(key) else {
            anyhow::bail!("tried to close document that didn't exist at {key}")
        };

        Ok(())
    }

    pub(super) fn document_mut(
        &mut self,
        key: &DocumentKey,
    ) -> Result<&mut Document, DocumentError> {
        let Some(controller) = self.documents.get_mut(key) else {
            return Err(DocumentError::NotFound(key.clone()));
        };
        Ok(controller)
    }
}

/// A mutable handler to an underlying document.
#[derive(Debug)]
pub(crate) enum Document {
    Text(Arc<TextDocument>),
    Notebook(Arc<NotebookDocument>),
}

impl Document {
    pub(super) fn new_text(document: TextDocument) -> Self {
        Self::Text(Arc::new(document))
    }

    pub(super) fn new_notebook(document: NotebookDocument) -> Self {
        Self::Notebook(Arc::new(document))
    }

    pub(crate) fn version(&self) -> DocumentVersion {
        match self {
            Self::Text(document) => document.version(),
            Self::Notebook(notebook) => notebook.version(),
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

#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum DocumentError {
    #[error("document not found for key: {0}")]
    NotFound(DocumentKey),
}
