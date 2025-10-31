use anyhow::Ok;
use lsp_types::NotebookCellKind;
use ruff_notebook::CellMetadata;
use rustc_hash::FxHashMap;

use super::{DocumentKey, DocumentVersion};
use crate::session::index::Index;

pub(super) type CellId = usize;

/// The state of a notebook document in the server. Contains an array of cells whose
/// contents are internally represented by [`TextDocument`]s.
#[derive(Clone, Debug)]
pub struct NotebookDocument {
    url: lsp_types::Url,
    cells: Vec<NotebookCell>,
    metadata: ruff_notebook::RawNotebookMetadata,
    version: DocumentVersion,
    /// Map from Cell URL to their index in `cells`
    cell_index: FxHashMap<lsp_types::Url, usize>,
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
}

impl NotebookDocument {
    pub fn new(
        url: lsp_types::Url,
        notebook_version: DocumentVersion,
        cells: Vec<lsp_types::NotebookCell>,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) -> crate::Result<Self> {
        let cells: Vec<_> = cells.into_iter().map(NotebookCell::new).collect();
        let index = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| (cell.url.clone(), index))
            .collect();

        Ok(Self {
            cell_index: index,
            url,
            version: notebook_version,
            cells,
            metadata: serde_json::from_value(serde_json::Value::Object(metadata))?,
        })
    }

    pub(crate) fn url(&self) -> &lsp_types::Url {
        &self.url
    }

    /// Generates a pseudo-representation of a notebook that lacks per-cell metadata and contextual information
    /// but should still work with Ruff's linter.
    pub(crate) fn to_ruff_notebook(&self, index: &Index) -> ruff_notebook::Notebook {
        let cells = self
            .cells
            .iter()
            .map(|cell| {
                let source = ruff_notebook::SourceValue::String(
                    index
                        .document(&DocumentKey::from_url(&cell.url))
                        .ok()
                        .and_then(|document| document.as_text())
                        .map(|text| text.contents().to_string())
                        .unwrap_or_default(),
                );
                match cell.kind {
                    NotebookCellKind::Code => ruff_notebook::Cell::Code(ruff_notebook::CodeCell {
                        execution_count: None,
                        id: None,
                        metadata: CellMetadata::default(),
                        outputs: vec![],
                        source,
                    }),
                    NotebookCellKind::Markup => {
                        ruff_notebook::Cell::Markdown(ruff_notebook::MarkdownCell {
                            attachments: None,
                            id: None,
                            metadata: CellMetadata::default(),
                            source,
                        })
                    }
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
        array: lsp_types::NotebookCellArrayChange,
        cells: Vec<lsp_types::NotebookCell>,
        metadata_change: Option<serde_json::Map<String, serde_json::Value>>,
        version: DocumentVersion,
    ) -> crate::Result<()> {
        self.version = version;

        let start = array.start as usize;
        let delete = array.delete_count as usize;
        let added = array
            .cells
            .as_ref()
            .map(|cells| cells.len())
            .unwrap_or_default();

        self.cells.drain(start..start + delete);

        // Second, insert the new cells with the available information. This array does not
        // provide the actual contents of the cells, so we'll initialize them with empty
        // contents.
        self.cells
            .extend(array.cells.into_iter().flatten().map(NotebookCell::new));

        // Re-build the cell-index if new cells were added, deleted or removed
        if start > 0 || delete > 0 || added > 0 {
            self.cell_index.clear();
            self.cell_index.extend(
                self.cells
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| (cell.url.clone(), i)),
            );
        }

        if !cells.is_empty() {
            for cell in cells {
                if let Some(existing_cell_index) = self.cell_index.get(&cell.document).copied() {
                    self.cells[existing_cell_index].kind = cell.kind;
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

    /// Returns a list of cell URIs in the order they appear in the array.
    pub(crate) fn cell_urls(&self) -> impl Iterator<Item = &lsp_types::Url> {
        self.cells.iter().map(|cell| &cell.url)
    }

    pub(crate) fn cell_index_by_uri(&self, cell_url: &lsp_types::Url) -> Option<CellId> {
        self.cell_index.get(cell_url).copied()
    }
}

impl NotebookCell {
    pub(crate) fn new(cell: lsp_types::NotebookCell) -> Self {
        Self {
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
        )
        .unwrap()
    }

    //     /// This test case checks that for a notebook with three code cells, when the client sends a
    //     /// change request to swap the first two cells, the notebook document is updated correctly.
    //     ///
    //     /// The swap operation as a change request is represented as deleting the first two cells and
    //     /// adding them back in the reverse order.
    //     #[test]
    //     fn swap_cells() {
    //         let mut notebook = create_test_notebook(vec![
    //             TestCellContent::Code("cell = 0".to_owned()),
    //             TestCellContent::Code("cell = 1".to_owned()),
    //             TestCellContent::Code("cell = 2".to_owned()),
    //         ]);
    //
    //         notebook
    //             .update(
    //                 Some(lsp_types::NotebookDocumentCellChange {
    //                     structure: Some(lsp_types::NotebookDocumentCellChangeStructure {
    //                         array: lsp_types::NotebookCellArrayChange {
    //                             start: 0,
    //                             delete_count: 2,
    //                             cells: Some(vec![
    //                                 lsp_types::NotebookCell {
    //                                     kind: lsp_types::NotebookCellKind::Code,
    //                                     document: create_test_url(1),
    //                                     metadata: None,
    //                                     execution_summary: None,
    //                                 },
    //                                 lsp_types::NotebookCell {
    //                                     kind: lsp_types::NotebookCellKind::Code,
    //                                     document: create_test_url(0),
    //                                     metadata: None,
    //                                     execution_summary: None,
    //                                 },
    //                             ]),
    //                         },
    //                         did_open: None,
    //                         did_close: None,
    //                     }),
    //                     data: None,
    //                     text_content: None,
    //                 }),
    //                 None,
    //                 1,
    //             )
    //             .unwrap();
    //
    //         assert_eq!(
    //             notebook.make_ruff_notebook().source_code(),
    //             "cell = 1
    // cell = 0
    // cell = 2
    // "
    //         );
    //     }
}
