use std::{collections::HashMap, hash::BuildHasherDefault};

use anyhow::Ok;
use lsp_types::{NotebookCellKind, Url};
use rustc_hash::FxHashMap;

use crate::{PositionEncoding, TextDocument};

use super::DocumentVersion;

/// The state of a notebook document in the server. Contains an array of cells whose
/// contents are internally represented by [`TextDocument`]s.
#[derive(Clone, Debug)]
pub(crate) struct NotebookDocument {
    cells: Vec<NotebookCell>,
    metadata: ruff_notebook::RawNotebookMetadata,
    version: DocumentVersion,
    // Used to quickly find the index of a cell for a given URL.
    cell_index: FxHashMap<lsp_types::Url, usize>,
}

#[derive(Clone, Debug)]
struct NotebookCell {
    url: Url,
    kind: NotebookCellKind,
    document: TextDocument,
}

impl NotebookDocument {
    pub(crate) fn new(
        version: DocumentVersion,
        cells: Vec<lsp_types::NotebookCell>,
        metadata: serde_json::Map<String, serde_json::Value>,
        cell_documents: Vec<lsp_types::TextDocumentItem>,
    ) -> crate::Result<Self> {
        let mut cell_contents: FxHashMap<_, _> = cell_documents
            .into_iter()
            .map(|document| (document.uri, document.text))
            .collect();

        let cells: Vec<_> = cells
            .into_iter()
            .map(|cell| {
                let contents = cell_contents.remove(&cell.document).unwrap_or_default();
                NotebookCell::new(cell, contents, version)
            })
            .collect();

        Ok(Self {
            version,
            cell_index: Self::make_cell_index(cells.as_slice()),
            metadata: serde_json::from_value(serde_json::Value::Object(metadata))?,
            cells,
        })
    }

    /// Generates a pseudo-representation of a notebook that lacks per-cell metadata and contextual information
    /// but should still work with Ruff's linter.
    pub(crate) fn make_ruff_notebook(&self) -> ruff_notebook::Notebook {
        let cells = self
            .cells
            .iter()
            .map(|cell| match cell.kind {
                NotebookCellKind::Code => ruff_notebook::Cell::Code(ruff_notebook::CodeCell {
                    execution_count: None,
                    id: None,
                    metadata: serde_json::Value::Null,
                    outputs: vec![],
                    source: ruff_notebook::SourceValue::String(
                        cell.document.contents().to_string(),
                    ),
                }),
                NotebookCellKind::Markup => {
                    ruff_notebook::Cell::Markdown(ruff_notebook::MarkdownCell {
                        attachments: None,
                        id: None,
                        metadata: serde_json::Value::Null,
                        source: ruff_notebook::SourceValue::String(
                            cell.document.contents().to_string(),
                        ),
                    })
                }
            })
            .collect();

        ruff_notebook::Notebook::from_cells(cells, self.metadata.clone())
            .expect("notebook should convert successfully")
    }

    pub(crate) fn update(
        &mut self,
        cells: Option<lsp_types::NotebookDocumentCellChange>,
        metadata_change: Option<serde_json::Map<String, serde_json::Value>>,
        version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> crate::Result<()> {
        self.version = version;

        if let Some(lsp_types::NotebookDocumentCellChange {
            structure,
            data,
            text_content,
        }) = cells
        {
            if let Some(structure) = structure {
                let start = usize::try_from(structure.array.start).unwrap();
                let delete = usize::try_from(structure.array.delete_count).unwrap();
                if delete > 0 {
                    self.cells.drain(start..start + delete);
                }
                for cell in structure.array.cells.into_iter().flatten().rev() {
                    self.cells
                        .insert(start, NotebookCell::new(cell, String::new(), version));
                }

                // the array has been updated - rebuild the cell index
                self.rebuild_cell_index();
            }
            if let Some(cell_data) = data {
                for cell in cell_data {
                    if let Some(existing_cell) = self.cell_by_uri_mut(&cell.document) {
                        existing_cell.kind = cell.kind;
                    }
                }
            }
            if let Some(content_changes) = text_content {
                for content_change in content_changes {
                    if let Some(cell) = self.cell_by_uri_mut(&content_change.document.uri) {
                        cell.document
                            .apply_changes(content_change.changes, version, encoding);
                    }
                }
            }
        }
        if let Some(metadata_change) = metadata_change {
            self.metadata = serde_json::from_value(serde_json::Value::Object(metadata_change))?;
        }
        Ok(())
    }

    pub(crate) fn version(&self) -> DocumentVersion {
        self.version
    }

    pub(crate) fn cell_uri_by_index(&self, index: usize) -> Option<&lsp_types::Url> {
        self.cells.get(index).map(|cell| &cell.url)
    }

    pub(crate) fn cell_document_by_uri(&self, uri: &lsp_types::Url) -> Option<&TextDocument> {
        self.cells
            .get(*self.cell_index.get(uri)?)
            .map(|cell| &cell.document)
    }

    pub(crate) fn urls(&self) -> impl Iterator<Item = &lsp_types::Url> {
        self.cells.iter().map(|cell| &cell.url)
    }

    fn cell_by_uri_mut(&mut self, uri: &lsp_types::Url) -> Option<&mut NotebookCell> {
        self.cells.get_mut(*self.cell_index.get(uri)?)
    }

    fn rebuild_cell_index(&mut self) {
        self.cell_index = Self::make_cell_index(&self.cells);
    }

    fn make_cell_index(cells: &[NotebookCell]) -> FxHashMap<lsp_types::Url, usize> {
        let mut index =
            HashMap::with_capacity_and_hasher(cells.len(), BuildHasherDefault::default());
        for (i, cell) in cells.iter().enumerate() {
            index.insert(cell.url.clone(), i);
        }
        index
    }
}

impl NotebookCell {
    pub(crate) fn new(
        cell: lsp_types::NotebookCell,
        contents: String,
        version: DocumentVersion,
    ) -> Self {
        Self {
            url: cell.document,
            kind: cell.kind,
            document: TextDocument::new(contents, version),
        }
    }
}
