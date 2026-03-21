use lsp_types::NotebookCellKind;
use ruff_notebook::CellMetadata;
use ruff_source_file::OneIndexed;
use rustc_hash::FxHashMap;

use super::{DocumentKey, DocumentVersion};
use crate::session::index::Index;

/// A notebook document.
///
/// This notebook document only stores the metadata about the notebook
/// and the cell metadata. The cell contents are stored as separate
/// [`super::TextDocument`]s (they can be looked up by the Cell's URL).
#[derive(Clone, Debug)]
pub struct NotebookDocument {
    url: lsp_types::Url,
    cells: Vec<NotebookCell>,
    metadata: ruff_notebook::RawNotebookMetadata,
    version: DocumentVersion,
    /// Map from Cell URL to their index in `cells`
    cell_index: FxHashMap<lsp_types::Url, usize>,
}

/// The metadata of a single cell within a notebook.
///
/// The cell's content are stored as a [`TextDocument`] and can be looked up by the Cell's URL.
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
    execution_summary: Option<lsp_types::ExecutionSummary>,
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
                let cell_text =
                    if let Ok(document) = index.document(&DocumentKey::from_url(&cell.url)) {
                        if let Some(text_document) = document.as_text() {
                            Some(text_document.contents().to_string())
                        } else {
                            tracing::warn!("Non-text document found for cell `{}`", cell.url);
                            None
                        }
                    } else {
                        tracing::warn!("Text document not found for cell `{}`", cell.url);
                        None
                    }
                    .unwrap_or_default();

                let source = ruff_notebook::SourceValue::String(cell_text);
                match cell.kind {
                    NotebookCellKind::Code => ruff_notebook::Cell::Code(ruff_notebook::CodeCell {
                        execution_count: cell
                            .execution_summary
                            .as_ref()
                            .map(|summary| i64::from(summary.execution_order)),
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
        updated_cells: Vec<lsp_types::NotebookCell>,
        metadata_change: Option<serde_json::Map<String, serde_json::Value>>,
        version: DocumentVersion,
    ) -> crate::Result<()> {
        self.version = version;

        let new_cells = array.cells.unwrap_or_default();
        let start = array.start as usize;

        let added = new_cells.len();
        let deleted_range = start..start + array.delete_count as usize;

        self.cells.splice(
            deleted_range.clone(),
            new_cells.into_iter().map(NotebookCell::new),
        );

        // Re-build the cell-index if new cells were added, deleted or removed
        if !deleted_range.is_empty() || added > 0 {
            self.cell_index.clear();
            self.cell_index.extend(
                self.cells
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| (cell.url.clone(), i)),
            );
        }

        for cell in updated_cells {
            if let Some(existing_cell_index) = self.cell_index.get(&cell.document).copied() {
                self.cells[existing_cell_index].kind = cell.kind;
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
    pub(crate) fn cell_uri_by_index(&self, index: OneIndexed) -> Option<&lsp_types::Url> {
        self.cells
            .get(index.to_zero_indexed())
            .map(|cell| &cell.url)
    }

    /// Returns a list of cell URIs in the order they appear in the array.
    pub(crate) fn cell_urls(&self) -> impl Iterator<Item = &lsp_types::Url> {
        self.cells.iter().map(|cell| &cell.url)
    }

    pub(crate) fn cell_index_by_uri(&self, cell_url: &lsp_types::Url) -> Option<OneIndexed> {
        Some(OneIndexed::from_zero_indexed(
            self.cell_index.get(cell_url).copied()?,
        ))
    }
}

impl NotebookCell {
    pub(crate) fn new(cell: lsp_types::NotebookCell) -> Self {
        Self {
            url: cell.document,
            kind: cell.kind,
            execution_summary: cell.execution_summary,
        }
    }
}
