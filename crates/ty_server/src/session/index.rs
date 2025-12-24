use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::document::{DocumentKey, LanguageId};
use crate::session::DocumentHandle;
use crate::{
    PositionEncoding, TextDocument,
    document::{DocumentVersion, NotebookDocument},
};

/// Stores and tracks all open documents in a session, along with their associated settings.
#[derive(Debug)]
pub(crate) struct Index {
    /// Maps all document file paths to the associated document controller
    documents: FxHashMap<DocumentKey, Document>,
}

impl Index {
    pub(super) fn new() -> Self {
        Self {
            documents: FxHashMap::default(),
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

        Ok(DocumentHandle::from_document(document))
    }

    #[expect(dead_code)]
    pub(super) fn notebook_document_keys(&self) -> impl Iterator<Item = &DocumentKey> + '_ {
        self.documents
            .iter()
            .filter(|(_, doc)| doc.as_notebook().is_some())
            .map(|(key, _)| key)
    }

    pub(super) fn update_notebook_document(
        &mut self,
        notebook_key: &DocumentKey,
        cells: Option<lsp_types::NotebookDocumentCellChange>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> crate::Result<()> {
        let document = self.document_mut(notebook_key)?;
        let Some(notebook) = document.as_notebook_mut() else {
            anyhow::bail!("Notebook document path does not point to a notebook document");
        };

        let (structure, data, text_content) = cells
            .map(|cells| {
                let lsp_types::NotebookDocumentCellChange {
                    structure,
                    data,
                    text_content,
                } = cells;
                (structure, data, text_content)
            })
            .unwrap_or_default();

        let (array, did_open, did_close) = structure
            .map(|structure| {
                let lsp_types::NotebookDocumentCellChangeStructure {
                    array,
                    did_open,
                    did_close,
                } = structure;

                (array, did_open, did_close)
            })
            .unwrap_or_else(|| {
                (
                    lsp_types::NotebookCellArrayChange {
                        start: 0,
                        delete_count: 0,
                        cells: None,
                    },
                    None,
                    None,
                )
            });

        tracing::info!(
            "version: {}, new_version: {}",
            notebook.version(),
            new_version
        );
        notebook.update(array, data.unwrap_or_default(), metadata, new_version)?;

        let notebook_path = notebook_key.to_file_path();

        for opened_cell in did_open.into_iter().flatten() {
            self.documents.insert(
                DocumentKey::from_url(&opened_cell.uri),
                Document::Text(
                    TextDocument::new(opened_cell.uri, opened_cell.text, opened_cell.version)
                        .with_language_id(&opened_cell.language_id)
                        .with_notebook(notebook_path.clone())
                        .into(),
                ),
            );
        }

        for updated_cell in text_content.into_iter().flatten() {
            let Ok(document_mut) =
                self.document_mut(&DocumentKey::from_url(&updated_cell.document.uri))
            else {
                tracing::warn!(
                    "Could not find document for cell {}",
                    updated_cell.document.uri
                );
                continue;
            };

            let Some(document) = document_mut.as_text_mut() else {
                continue;
            };

            if updated_cell.changes.is_empty() {
                document.update_version(updated_cell.document.version);
            } else {
                document.apply_changes(
                    updated_cell.changes,
                    updated_cell.document.version,
                    encoding,
                );
            }
        }

        // VS Code sends a separate `didClose` request for every cell
        // and they're removed from the metadata (notebook document)
        // because they get deleted as part of `change.cells.structure.array`
        let _ = did_close;

        let notebook = self.document(notebook_key).unwrap().as_notebook().unwrap();
        let ruff_notebook = notebook.to_ruff_notebook(self);
        tracing::debug!("Updated notebook: {:?}", ruff_notebook.source_code());

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

    pub(super) fn open_text_document(&mut self, document: TextDocument) -> DocumentHandle {
        let key = DocumentKey::from_url(document.url());

        let handle = DocumentHandle::from_text_document(&document);

        self.documents.insert(key, Document::new_text(document));

        handle
    }

    pub(super) fn open_notebook_document(&mut self, document: NotebookDocument) -> DocumentHandle {
        let handle = DocumentHandle::from_notebook_document(&document);
        let notebook_key = DocumentKey::from_url(document.url());

        self.documents
            .insert(notebook_key, Document::new_notebook(document));

        handle
    }

    pub(super) fn close_document(&mut self, key: &DocumentKey) -> Result<Document, DocumentError> {
        let Some(document) = self.documents.remove(key) else {
            return Err(DocumentError::NotFound(key.clone()));
        };

        Ok(document)
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

    pub(crate) fn language_id(&self) -> Option<LanguageId> {
        match self {
            Self::Text(document) => document.language_id(),
            Self::Notebook(_) => None,
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
