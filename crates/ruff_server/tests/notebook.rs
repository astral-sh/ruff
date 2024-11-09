use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use lsp_types::{
    ClientCapabilities, LSPObject, NotebookDocumentCellChange, NotebookDocumentChangeTextContent,
    Position, Range, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};
use ruff_notebook::SourceValue;
use ruff_server::{ClientSettings, Workspace, Workspaces};

const SUPER_RESOLUTION_OVERVIEW_PATH: &str =
    "./resources/test/fixtures/tensorflow_test_notebook.ipynb";

struct NotebookChange {
    version: i32,
    metadata: Option<LSPObject>,
    updated_cells: lsp_types::NotebookDocumentCellChange,
}

#[test]
fn super_resolution_overview() {
    let file_path =
        std::fs::canonicalize(PathBuf::from_str(SUPER_RESOLUTION_OVERVIEW_PATH).unwrap()).unwrap();
    let file_url = lsp_types::Url::from_file_path(&file_path).unwrap();
    let notebook = create_notebook(&file_path).unwrap();

    insta::assert_snapshot!("initial_notebook", notebook_source(&notebook));

    let mut session = ruff_server::Session::new(
        &ClientCapabilities::default(),
        ruff_server::PositionEncoding::UTF16,
        ClientSettings::default(),
        &Workspaces::new(vec![Workspace::new(
            lsp_types::Url::from_file_path(file_path.parent().unwrap()).unwrap(),
        )
        .with_settings(ClientSettings::default())]),
    )
    .unwrap();

    session.open_notebook_document(file_url.clone(), notebook);

    let changes = [NotebookChange {
        version: 0,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 5),
                    version: 2,
                },
                changes: vec![
                    TextDocumentContentChangeEvent {
                        range: Some(Range {
                            start: Position {
                                line: 18,
                                character: 61,
                            },
                            end: Position {
                                line: 18,
                                character: 62,
                            },
                        }),
                        range_length: Some(1),
                        text: "\"".to_string(),
                    },
                    TextDocumentContentChangeEvent {
                        range: Some(Range {
                            start: Position {
                                line: 18,
                                character: 55,
                            },
                            end: Position {
                                line: 18,
                                character: 56,
                            },
                        }),
                        range_length: Some(1),
                        text: "\"".to_string(),
                    },
                    TextDocumentContentChangeEvent {
                        range: Some(Range {
                            start: Position {
                                line: 14,
                                character: 46,
                            },
                            end: Position {
                                line: 14,
                                character: 47,
                            },
                        }),
                        range_length: Some(1),
                        text: "\"".to_string(),
                    },
                    TextDocumentContentChangeEvent {
                        range: Some(Range {
                            start: Position {
                                line: 14,
                                character: 40,
                            },
                            end: Position {
                                line: 14,
                                character: 41,
                            },
                        }),
                        range_length: Some(1),
                        text: "\"".to_string(),
                    },
                ],
            }]),
        },
    },
    NotebookChange {
        version: 1,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 4),
                    version: 2
                },
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 0,
                            character: 0
                        },
                        end: Position {
                            line: 0,
                            character: 181
                        } }),
                        range_length: Some(181),
                        text: "test_img_path = tf.keras.utils.get_file(\n    \"lr.jpg\",\n    \"https://raw.githubusercontent.com/tensorflow/examples/master/lite/examples/super_resolution/android/app/src/main/assets/lr-1.jpg\",\n)".to_string()
                    }
                    ]
                }
                ]
            )
        }
    },
    NotebookChange {
        version: 2,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 2),
                    version: 2,
                },
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 3,
                            character: 0,
                        },
                        end: Position {
                            line: 3,
                            character: 21,
                        },
                    }),
                    range_length: Some(21),
                    text: "\nprint(tf.__version__)".to_string(),
                }],
            }]),
        }
    },
    NotebookChange {
        version: 3,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 1),
                    version: 2,
                },
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 49,
                        },
                    }),
                    range_length: Some(49),
                    text: "!pip install matplotlib tensorflow tensorflow-hub".to_string(),
                }],
            }]),
        },
    },
    NotebookChange {
        version: 4,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 3),
                    version: 2,
                },
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 3,
                            character: 0,
                        },
                        end: Position {
                            line: 15,
                            character: 37,
                        },
                    }),
                    range_length: Some(457),
                    text: "\n@tf.function(input_signature=[tf.TensorSpec(shape=[1, 50, 50, 3], dtype=tf.float32)])\ndef f(input):\n    return concrete_func(input)\n\n\nconverter = tf.lite.TFLiteConverter.from_concrete_functions(\n    [f.get_concrete_function()], model\n)\nconverter.optimizations = [tf.lite.Optimize.DEFAULT]\ntflite_model = converter.convert()\n\n# Save the TF Lite model.\nwith tf.io.gfile.GFile(\"ESRGAN.tflite\", \"wb\") as f:\n    f.write(tflite_model)\n\nesrgan_model_path = \"./ESRGAN.tflite\"".to_string(),
                }],
            }]),
        },
    },
    NotebookChange {
        version: 5,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 0),
                    version: 2,
                },
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 2,
                            character: 0,
                        },
                    }),
                    range_length: Some(139),
                    text: "# @title Licensed under the Apache License, Version 2.0 (the \"License\");\n# you may not use this file except in compliance with the License.\n".to_string(),
                }],
            }]),
        },
    },
    NotebookChange {
        version: 6,
        metadata: None,
        updated_cells: NotebookDocumentCellChange {
            structure: None,
            data: None,
            text_content: Some(vec![NotebookDocumentChangeTextContent {
                document: VersionedTextDocumentIdentifier {
                    uri: make_cell_uri(&file_path, 6),
                    version: 2,
                },
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position {
                            line: 1,
                            character: 0,
                        },
                        end: Position {
                            line: 14,
                            character: 28,
                        },
                    }),
                    range_length: Some(361),
                    text: "plt.figure(figsize=(1, 1))\nplt.title(\"LR\")\nplt.imshow(lr.numpy())\nplt.figure(figsize=(10, 4))\nplt.subplot(1, 2, 1)\nplt.title(f\"ESRGAN (x4)\")\nplt.imshow(sr.numpy())\nbicubic = tf.image.resize(lr, [200, 200], tf.image.ResizeMethod.BICUBIC)\nbicubic = tf.cast(bicubic, tf.uint8)\nplt.subplot(1, 2, 2)\nplt.title(\"Bicubic\")\nplt.imshow(bicubic.numpy());".to_string(),
                }],
            }]),
        },
    }
    ];

    let key = session.key_from_url(file_url.clone());

    for NotebookChange {
        version,
        metadata,
        updated_cells,
    } in changes
    {
        session
            .update_notebook_document(&key, Some(updated_cells), metadata, version)
            .unwrap();
    }

    let snapshot = session.take_snapshot(file_url).unwrap();

    insta::assert_snapshot!(
        "changed_notebook",
        notebook_source(snapshot.query().as_notebook().unwrap())
    );
}

fn notebook_source(notebook: &ruff_server::NotebookDocument) -> String {
    notebook.make_ruff_notebook().source_code().to_string()
}

// produces an opaque URL based on a document path and a cell index
fn make_cell_uri(path: &Path, index: usize) -> lsp_types::Url {
    lsp_types::Url::parse(&format!(
        "notebook-cell:///Users/test/notebooks/{}.ipynb?cell={index}",
        path.file_name().unwrap().to_string_lossy()
    ))
    .unwrap()
}

fn create_notebook(file_path: &Path) -> anyhow::Result<ruff_server::NotebookDocument> {
    let ruff_notebook = ruff_notebook::Notebook::from_path(file_path)?;

    let mut cells = vec![];
    let mut cell_documents = vec![];
    for (i, cell) in ruff_notebook
        .cells()
        .iter()
        .filter(|cell| cell.is_code_cell())
        .enumerate()
    {
        let uri = make_cell_uri(file_path, i);
        let (lsp_cell, cell_document) = cell_to_lsp_cell(cell, uri)?;
        cells.push(lsp_cell);
        cell_documents.push(cell_document);
    }

    let serde_json::Value::Object(metadata) = serde_json::to_value(ruff_notebook.metadata())?
    else {
        anyhow::bail!("Notebook metadata was not an object");
    };

    ruff_server::NotebookDocument::new(0, cells, metadata, cell_documents)
}

fn cell_to_lsp_cell(
    cell: &ruff_notebook::Cell,
    cell_uri: lsp_types::Url,
) -> anyhow::Result<(lsp_types::NotebookCell, lsp_types::TextDocumentItem)> {
    let contents = match cell.source() {
        SourceValue::String(string) => string.clone(),
        SourceValue::StringArray(array) => array.join(""),
    };
    let metadata = match serde_json::to_value(cell.metadata())? {
        serde_json::Value::Null => None,
        serde_json::Value::Object(metadata) => Some(metadata),
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
        lsp_types::TextDocumentItem::new(cell_uri, "python".to_string(), 1, contents),
    ))
}
