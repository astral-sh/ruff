use std::path::{Path, PathBuf};

use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::{
    DidChangeNotebookDocumentNotification, DidOpenNotebookDocumentNotification,
    NotebookDocumentCellContentChanges, TextDocumentContentChangePartial, TextDocumentIdentifier,
};
use lsp_types::{
    DidChangeNotebookDocumentParams, DidOpenNotebookDocumentParams, LspObject, NotebookDocument,
    NotebookDocumentCellChanges, NotebookDocumentChangeEvent, Position, Range,
    TextDocumentContentChangeEvent, TextDocumentItem, VersionedNotebookDocumentIdentifier,
    VersionedTextDocumentIdentifier,
};
use ruff_notebook::SourceValue;

use crate::TestServerBuilder;

const NOTEBOOK_FIXTURE_PATH: &str = "resources/test/fixtures/tensorflow_test_notebook.ipynb";

struct NotebookChange {
    version: i32,
    metadata: Option<LspObject>,
    updated_cells: NotebookDocumentCellChanges,
}

#[test]
fn super_resolution_overview() -> Result<()> {
    let fixture_path = fixture_path(NOTEBOOK_FIXTURE_PATH)?;
    let workspace_dir = fixture_path
        .parent()
        .expect("notebook fixture should have a parent");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_dir)?
        .build();

    let (notebook_document, cell_text_documents) =
        create_lsp_notebook(&fixture_path, fixture_path.clone())?;
    let notebook_uri = notebook_document.uri.clone();
    let cell_count = cell_text_documents.len();

    server.send_notification::<DidOpenNotebookDocumentNotification>(
        DidOpenNotebookDocumentParams {
            notebook_document,
            cell_text_documents,
        },
    );

    let diagnostics = server.collect_publish_diagnostic_notifications(cell_count);
    assert_json_snapshot!("super_resolution_overview_open", diagnostics);

    let changes = [
        NotebookChange {
            version: 0,
            metadata: None,
            updated_cells: NotebookDocumentCellChanges {
                structure: None,
                data: None,
                text_content: Some(vec![NotebookDocumentCellContentChanges {
                    document: VersionedTextDocumentIdentifier {
                        text_document_identifier: TextDocumentIdentifier {
                            uri: make_cell_uri(&fixture_path, 5),
                        },
                        version: 2,
                    },
                    changes: vec![
                        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                            TextDocumentContentChangePartial {
                                range: Range {
                                    start: Position {
                                        line: 18,
                                        character: 61,
                                    },
                                    end: Position {
                                        line: 18,
                                        character: 62,
                                    },
                                },
                                text: "\"".to_string(),
                                ..Default::default()
                            },
                        ),
                        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                            TextDocumentContentChangePartial {
                                range: Range {
                                    start: Position {
                                        line: 18,
                                        character: 55,
                                    },
                                    end: Position {
                                        line: 18,
                                        character: 56,
                                    },
                                },
                                text: "\"".to_string(),
                                ..Default::default()
                            },
                        ),
                        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                            TextDocumentContentChangePartial {
                                range: Range {
                                    start: Position {
                                        line: 14,
                                        character: 46,
                                    },
                                    end: Position {
                                        line: 14,
                                        character: 47,
                                    },
                                },
                                text: "\"".to_string(),
                                ..Default::default()
                            },
                        ),
                        TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                            TextDocumentContentChangePartial {
                                range: Range {
                                    start: Position {
                                        line: 14,
                                        character: 40,
                                    },
                                    end: Position {
                                        line: 14,
                                        character: 41,
                                    },
                                },
                                text: "\"".to_string(),
                                ..Default::default()
                            },
                        ),
                    ],
                }]),
            },
        },
        NotebookChange {
            version: 1,
            metadata: None,
            updated_cells: NotebookDocumentCellChanges {
                structure: None,
                data: None,
                text_content: Some(vec![NotebookDocumentCellContentChanges {
                    document: VersionedTextDocumentIdentifier {
                        text_document_identifier: TextDocumentIdentifier {
                            uri: make_cell_uri(&fixture_path, 4),
                        },
                        version: 2,
                    },
                    changes: vec![TextDocumentContentChangeEvent::TextDocumentContentChangePartial(TextDocumentContentChangePartial {
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 181,
                            },
                        },
                        text: "test_img_path = tf.keras.utils.get_file(\n    \"lr.jpg\",\n    \"https://raw.githubusercontent.com/tensorflow/examples/master/lite/examples/super_resolution/android/app/src/main/assets/lr-1.jpg\",\n)".to_string(),
                        ..Default::default()
                    })],
                }]),
            },
        },
    ];

    let mut final_diagnostics = None;

    for NotebookChange {
        version,
        metadata,
        updated_cells,
    } in changes
    {
        server.send_notification::<DidChangeNotebookDocumentNotification>(
            DidChangeNotebookDocumentParams {
                notebook_document: VersionedNotebookDocumentIdentifier {
                    uri: notebook_uri.clone(),
                    version,
                },
                change: NotebookDocumentChangeEvent {
                    metadata,
                    cells: Some(updated_cells),
                },
            },
        );

        final_diagnostics = Some(server.collect_publish_diagnostic_notifications(cell_count));
    }

    assert_json_snapshot!(
        "super_resolution_overview_final",
        final_diagnostics.expect("at least one notebook change")
    );

    Ok(())
}

#[test]
fn notebook_without_ipynb_extension() -> Result<()> {
    let fixture_path = fixture_path(NOTEBOOK_FIXTURE_PATH)?;
    let workspace_dir = fixture_path
        .parent()
        .expect("notebook fixture should have a parent");

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_dir)?
        .build();

    let (notebook_document, cell_text_documents) =
        create_lsp_notebook(&fixture_path, workspace_dir.join("notebook.py"))?;
    let cell_count = cell_text_documents.len();

    server.send_notification::<DidOpenNotebookDocumentNotification>(
        DidOpenNotebookDocumentParams {
            notebook_document,
            cell_text_documents,
        },
    );

    let diagnostics = server.collect_publish_diagnostic_notifications(cell_count);
    assert_json_snapshot!("notebook_without_ipynb_extension_open", diagnostics);

    Ok(())
}

fn fixture_path(path: &str) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
    )?)
}

fn create_lsp_notebook(
    file_path: &Path,
    open_uri_path: PathBuf,
) -> Result<(NotebookDocument, Vec<TextDocumentItem>)> {
    let notebook = ruff_notebook::Notebook::from_path(file_path)?;
    let notebook_uri = lsp_types::Uri::from_file_path(open_uri_path).unwrap();

    let mut cells = Vec::new();
    let mut cell_text_documents = Vec::new();

    for (index, cell) in notebook
        .cells()
        .iter()
        .filter(|cell| cell.is_code_cell())
        .enumerate()
    {
        let uri = make_cell_uri(file_path, index);
        let (lsp_cell, text_document) = cell_to_lsp_cell(cell, uri)?;
        cells.push(lsp_cell);
        cell_text_documents.push(text_document);
    }

    Ok((
        NotebookDocument {
            uri: notebook_uri,
            notebook_type: "jupyter-notebook".to_string(),
            version: 0,
            metadata: None,
            cells,
        },
        cell_text_documents,
    ))
}

fn make_cell_uri(path: &Path, index: usize) -> lsp_types::Uri {
    lsp_types::Uri::parse(&format!(
        "notebook-cell:///Users/test/notebooks/{}.ipynb?cell={index}",
        path.file_name().unwrap().to_string_lossy()
    ))
    .unwrap()
}

fn cell_to_lsp_cell(
    cell: &ruff_notebook::Cell,
    cell_uri: lsp_types::Uri,
) -> Result<(lsp_types::NotebookCell, TextDocumentItem)> {
    let contents = match cell.source() {
        SourceValue::String(string) => string.clone(),
        SourceValue::StringArray(array) => array.join(""),
    };
    let metadata = match serde_json::to_value(cell.metadata())? {
        serde_json::Value::Null => None,
        metadata @ serde_json::Value::Object(_) => Some(serde_json::from_value(metadata)?),
        _ => anyhow::bail!("Notebook cell metadata was not an object"),
    };
    Ok((
        lsp_types::NotebookCell {
            kind: match cell {
                ruff_notebook::Cell::Code(_) => lsp_types::NotebookCellKind::Code,
                ruff_notebook::Cell::Markdown(_) => lsp_types::NotebookCellKind::Markup,
                ruff_notebook::Cell::Raw(_) => unreachable!(),
            },
            document: cell_uri.clone(),
            metadata,
            execution_summary: None,
        },
        TextDocumentItem::new(cell_uri, lsp_types::LanguageKind::Python, 0, contents),
    ))
}
