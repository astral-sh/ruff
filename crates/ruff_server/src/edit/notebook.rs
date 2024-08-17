use anyhow::Ok;
use lsp_types::NotebookCellKind;
use ruff_notebook::CellMetadata;
use rustc_hash::{FxBuildHasher, FxHashMap};

use crate::{PositionEncoding, TextDocument};

use super::DocumentVersion;

pub(super) type CellId = usize;

/// The state of a notebook document in the server. Contains an array of cells whose
/// contents are internally represented by [`TextDocument`]s.
#[derive(Clone, Debug)]
pub struct NotebookDocument {
    cells: Vec<NotebookCell>,
    metadata: ruff_notebook::RawNotebookMetadata,
    version: DocumentVersion,
    // Used to quickly find the index of a cell for a given URL.
    cell_index: FxHashMap<lsp_types::Url, CellId>,
}

/// A single cell within a notebook, which has text contents represented as a `TextDocument`.
#[derive(Clone, Debug)]
struct NotebookCell {
    url: lsp_types::Url,
    kind: NotebookCellKind,
    document: TextDocument,
}

impl NotebookDocument {
    pub fn new(
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
    pub fn make_ruff_notebook(&self) -> ruff_notebook::Notebook {
        let cells = self
            .cells
            .iter()
            .map(|cell| match cell.kind {
                NotebookCellKind::Code => ruff_notebook::Cell::Code(ruff_notebook::CodeCell {
                    execution_count: None,
                    id: None,
                    metadata: CellMetadata::default(),
                    outputs: vec![],
                    source: ruff_notebook::SourceValue::String(
                        cell.document.contents().to_string(),
                    ),
                }),
                NotebookCellKind::Markup => {
                    ruff_notebook::Cell::Markdown(ruff_notebook::MarkdownCell {
                        attachments: None,
                        id: None,
                        metadata: CellMetadata::default(),
                        source: ruff_notebook::SourceValue::String(
                            cell.document.contents().to_string(),
                        ),
                    })
                }
            })
            .collect();
        let raw_notebook = ruff_notebook::RawNotebook {
            cells,
            metadata: self.metadata.clone(),
            nbformat: 4,
            nbformat_minor: 5,
        };

        ruff_notebook::Notebook::from_raw_notebook(raw_notebook, false)
            .unwrap_or_else(|err| panic!("Server notebook document could not be converted to Ruff's notebook document format: {err}"))
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
            // The structural changes should be done first, as they may affect the cell index.
            if let Some(structure) = structure {
                let start = structure.array.start as usize;
                let delete = structure.array.delete_count as usize;

                // This is required because of the way the `NotebookCell` is modelled. We include
                // the `TextDocument` within the `NotebookCell` so when it's deleted, the
                // corresponding `TextDocument` is removed as well. But, when cells are
                // re-oredered, the change request doesn't provide the actual contents of the cell.
                // Instead, it only provides that (a) these cell URIs were removed, and (b) these
                // cell URIs were added.
                // https://github.com/astral-sh/ruff/issues/12573
                let mut deleted_cells = FxHashMap::default();

                // First, delete the cells and remove them from the index.
                if delete > 0 {
                    for cell in self.cells.drain(start..start + delete) {
                        self.cell_index.remove(&cell.url);
                        deleted_cells.insert(cell.url, cell.document);
                    }
                }

                // Second, insert the new cells with the available information. This array does not
                // provide the actual contents of the cells, so we'll initialize them with empty
                // contents.
                for cell in structure.array.cells.into_iter().flatten().rev() {
                    if let Some(text_document) = deleted_cells.remove(&cell.document) {
                        let version = text_document.version();
                        self.cells.push(NotebookCell::new(
                            cell,
                            text_document.into_contents(),
                            version,
                        ));
                    } else {
                        self.cells
                            .insert(start, NotebookCell::new(cell, String::new(), 0));
                    }
                }

                // Third, register the new cells in the index and update existing ones that came
                // after the insertion.
                for (index, cell) in self.cells.iter().enumerate().skip(start) {
                    self.cell_index.insert(cell.url.clone(), index);
                }

                // Finally, update the text document that represents the cell with the actual
                // contents. This should be done at the end so that both the `cells` and
                // `cell_index` are updated before we start applying the changes to the cells.
                if let Some(did_open) = structure.did_open {
                    for cell_text_document in did_open {
                        if let Some(cell) = self.cell_by_uri_mut(&cell_text_document.uri) {
                            cell.document = TextDocument::new(
                                cell_text_document.text,
                                cell_text_document.version,
                            );
                        }
                    }
                }
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

    /// Get the current version of the notebook document.
    pub(crate) fn version(&self) -> DocumentVersion {
        self.version
    }

    /// Get the URI for a cell by its index within the cell array.
    pub(crate) fn cell_uri_by_index(&self, index: CellId) -> Option<&lsp_types::Url> {
        self.cells.get(index).map(|cell| &cell.url)
    }

    /// Get the text document representing the contents of a cell by the cell URI.
    pub(crate) fn cell_document_by_uri(&self, uri: &lsp_types::Url) -> Option<&TextDocument> {
        self.cells
            .get(*self.cell_index.get(uri)?)
            .map(|cell| &cell.document)
    }

    /// Returns a list of cell URIs in the order they appear in the array.
    pub(crate) fn urls(&self) -> impl Iterator<Item = &lsp_types::Url> {
        self.cells.iter().map(|cell| &cell.url)
    }

    fn cell_by_uri_mut(&mut self, uri: &lsp_types::Url) -> Option<&mut NotebookCell> {
        self.cells.get_mut(*self.cell_index.get(uri)?)
    }

    fn make_cell_index(cells: &[NotebookCell]) -> FxHashMap<lsp_types::Url, CellId> {
        let mut index = FxHashMap::with_capacity_and_hasher(cells.len(), FxBuildHasher);
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
