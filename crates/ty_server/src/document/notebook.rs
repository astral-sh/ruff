use anyhow::Ok;
use lsp_types::NotebookCellKind;
use ruff_notebook::CellMetadata;
use rustc_hash::{FxBuildHasher, FxHashMap};

use super::DocumentVersion;
use crate::{PositionEncoding, TextDocument};

pub(super) type CellId = usize;

/// The state of a notebook document in the server. Contains an array of cells whose
/// contents are internally represented by [`TextDocument`]s.
#[derive(Clone, Debug)]
pub struct NotebookDocument {
    url: lsp_types::Url,
    cells: Vec<NotebookCell>,
    metadata: ruff_notebook::RawNotebookMetadata,
    version: DocumentVersion,
    // Used to quickly find the index of a cell for a given URL.
    cell_index: FxHashMap<String, CellId>,
}

/// A single cell within a notebook, which has text contents represented as a `TextDocument`.
#[derive(Clone, Debug)]
struct NotebookCell {
    /// The URL uniquely identifying the cell.
    ///
    /// > Cell text documents have a URI, but servers should not rely on any
    /// > format for this URI, since it is up to the client on how it will
    /// > create these URIs. The URIs must be unique across ALL notebook
    /// > cells and can therefore be used to uniquely identify a notebook cell
    /// >  or the cell’s text document.
    /// > <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#notebookDocument_synchronization>
    url: lsp_types::Url,
    kind: NotebookCellKind,
    document: TextDocument,
}

impl NotebookDocument {
    pub fn new(
        url: lsp_types::Url,
        notebook_version: DocumentVersion,
        cells: Vec<lsp_types::NotebookCell>,
        metadata: serde_json::Map<String, serde_json::Value>,
        cell_documents: Vec<lsp_types::TextDocumentItem>,
    ) -> crate::Result<Self> {
        let mut cells: Vec<_> = cells.into_iter().map(NotebookCell::empty).collect();

        let cell_index = Self::make_cell_index(&cells);

        for cell_document in cell_documents {
            let index = cell_index
                .get(cell_document.uri.as_str())
                .copied()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Received content for cell `{}` that isn't present in the metadata",
                        cell_document.uri
                    )
                })?;

            cells[index].document =
                TextDocument::new(cell_document.uri, cell_document.text, cell_document.version)
                    .with_language_id(&cell_document.language_id);
        }

        Ok(Self {
            url,
            version: notebook_version,
            cell_index,
            cells,
            metadata: serde_json::from_value(serde_json::Value::Object(metadata))?,
        })
    }

    pub(crate) fn url(&self) -> &lsp_types::Url {
        &self.url
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
            .unwrap_or_else(|err| panic!("Server notebook document could not be converted to ty's notebook document format: {err}"))
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
                // re-ordered, the change request doesn't provide the actual contents of the cell.
                // Instead, it only provides that (a) these cell URIs were removed, and (b) these
                // cell URIs were added.
                // https://github.com/astral-sh/ruff/issues/12573
                let mut deleted_cells = FxHashMap::default();

                // First, delete the cells and remove them from the index.
                if delete > 0 {
                    for cell in self.cells.drain(start..start + delete) {
                        self.cell_index.remove(cell.url.as_str());
                        deleted_cells.insert(cell.url, cell.document);
                    }
                }

                // Second, insert the new cells with the available information. This array does not
                // provide the actual contents of the cells, so we'll initialize them with empty
                // contents.
                for cell in structure.array.cells.into_iter().flatten().rev() {
                    let (content, version) =
                        if let Some(text_document) = deleted_cells.remove(&cell.document) {
                            let version = text_document.version();
                            (text_document.into_contents(), version)
                        } else {
                            (String::new(), 0)
                        };
                    self.cells
                        .insert(start, NotebookCell::new(cell, content, version));
                }

                // Third, register the new cells in the index and update existing ones that came
                // after the insertion.
                for (index, cell) in self.cells.iter().enumerate().skip(start) {
                    self.cell_index.insert(cell.url.to_string(), index);
                }

                // Finally, update the text document that represents the cell with the actual
                // contents. This should be done at the end so that both the `cells` and
                // `cell_index` are updated before we start applying the changes to the cells.
                if let Some(did_open) = structure.did_open {
                    for cell_text_document in did_open {
                        if let Some(cell) = self.cell_by_uri_mut(cell_text_document.uri.as_str()) {
                            cell.document = TextDocument::new(
                                cell_text_document.uri,
                                cell_text_document.text,
                                cell_text_document.version,
                            );
                        }
                    }
                }
            }

            if let Some(cell_data) = data {
                for cell in cell_data {
                    if let Some(existing_cell) = self.cell_by_uri_mut(cell.document.as_str()) {
                        existing_cell.kind = cell.kind;
                    }
                }
            }

            if let Some(content_changes) = text_content {
                for content_change in content_changes {
                    if let Some(cell) = self.cell_by_uri_mut(content_change.document.uri.as_str()) {
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
    #[expect(unused)]
    pub(crate) fn cell_document_by_uri(&self, uri: &str) -> Option<&TextDocument> {
        self.cells
            .get(*self.cell_index.get(uri)?)
            .map(|cell| &cell.document)
    }

    /// Returns a list of cell URIs in the order they appear in the array.
    pub(crate) fn cell_urls(&self) -> impl Iterator<Item = &lsp_types::Url> {
        self.cells.iter().map(|cell| &cell.url)
    }

    fn cell_by_uri_mut(&mut self, uri: &str) -> Option<&mut NotebookCell> {
        self.cells.get_mut(*self.cell_index.get(uri)?)
    }

    fn make_cell_index(cells: &[NotebookCell]) -> FxHashMap<String, CellId> {
        let mut index = FxHashMap::with_capacity_and_hasher(cells.len(), FxBuildHasher);
        for (i, cell) in cells.iter().enumerate() {
            index.insert(cell.url.to_string(), i);
        }
        index
    }
}

impl NotebookCell {
    pub(crate) fn empty(cell: lsp_types::NotebookCell) -> Self {
        Self {
            kind: cell.kind,
            document: TextDocument::new(
                cell.document.clone(),
                String::new(),
                DocumentVersion::default(),
            ),
            url: cell.document,
        }
    }

    pub(crate) fn new(
        cell: lsp_types::NotebookCell,
        contents: String,
        version: DocumentVersion,
    ) -> Self {
        Self {
            document: TextDocument::new(cell.document.clone(), contents, version),
            url: cell.document,
            kind: cell.kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NotebookDocument;

    enum TestCellContent {
        #[expect(dead_code)]
        Markup(String),
        Code(String),
    }

    fn create_test_url(index: usize) -> lsp_types::Url {
        lsp_types::Url::parse(&format!("cell:/test.ipynb#{index}")).unwrap()
    }

    fn create_test_notebook(test_cells: Vec<TestCellContent>) -> NotebookDocument {
        let mut cells = Vec::with_capacity(test_cells.len());
        let mut cell_documents = Vec::with_capacity(test_cells.len());

        for (index, test_cell) in test_cells.into_iter().enumerate() {
            let url = create_test_url(index);
            match test_cell {
                TestCellContent::Markup(content) => {
                    cells.push(lsp_types::NotebookCell {
                        kind: lsp_types::NotebookCellKind::Markup,
                        document: url.clone(),
                        metadata: None,
                        execution_summary: None,
                    });
                    cell_documents.push(lsp_types::TextDocumentItem {
                        uri: url,
                        language_id: "markdown".to_owned(),
                        version: 0,
                        text: content,
                    });
                }
                TestCellContent::Code(content) => {
                    cells.push(lsp_types::NotebookCell {
                        kind: lsp_types::NotebookCellKind::Code,
                        document: url.clone(),
                        metadata: None,
                        execution_summary: None,
                    });
                    cell_documents.push(lsp_types::TextDocumentItem {
                        uri: url,
                        language_id: "python".to_owned(),
                        version: 0,
                        text: content,
                    });
                }
            }
        }

        NotebookDocument::new(
            lsp_types::Url::parse("file://test.ipynb").unwrap(),
            0,
            cells,
            serde_json::Map::default(),
            cell_documents,
        )
        .unwrap()
    }

    /// This test case checks that for a notebook with three code cells, when the client sends a
    /// change request to swap the first two cells, the notebook document is updated correctly.
    ///
    /// The swap operation as a change request is represented as deleting the first two cells and
    /// adding them back in the reverse order.
    #[test]
    fn swap_cells() {
        let mut notebook = create_test_notebook(vec![
            TestCellContent::Code("cell = 0".to_owned()),
            TestCellContent::Code("cell = 1".to_owned()),
            TestCellContent::Code("cell = 2".to_owned()),
        ]);

        notebook
            .update(
                Some(lsp_types::NotebookDocumentCellChange {
                    structure: Some(lsp_types::NotebookDocumentCellChangeStructure {
                        array: lsp_types::NotebookCellArrayChange {
                            start: 0,
                            delete_count: 2,
                            cells: Some(vec![
                                lsp_types::NotebookCell {
                                    kind: lsp_types::NotebookCellKind::Code,
                                    document: create_test_url(1),
                                    metadata: None,
                                    execution_summary: None,
                                },
                                lsp_types::NotebookCell {
                                    kind: lsp_types::NotebookCellKind::Code,
                                    document: create_test_url(0),
                                    metadata: None,
                                    execution_summary: None,
                                },
                            ]),
                        },
                        did_open: None,
                        did_close: None,
                    }),
                    data: None,
                    text_content: None,
                }),
                None,
                1,
                crate::PositionEncoding::default(),
            )
            .unwrap();

        assert_eq!(
            notebook.make_ruff_notebook().source_code(),
            "cell = 1
cell = 0
cell = 2
"
        );
    }
}
