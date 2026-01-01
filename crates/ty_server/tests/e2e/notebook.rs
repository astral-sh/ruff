use insta::assert_json_snapshot;
use lsp_types::{NotebookCellKind, Position, Range};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::{TestServer, TestServerBuilder};

static FILTERS: &[(&str, &str)] = &[(r#""sortText": "[0-9 ]+""#, r#""sortText": "[RANKING]""#)];

#[test]
fn publish_diagnostics_open() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .enable_diagnostic_related_information(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("test.ipynb");
    builder.add_python_cell(
        r#"from typing import Literal

type Style = Literal["italic", "bold", "underline"]"#,
    );

    builder.add_python_cell(
        r#"def with_style(line: str, word, style: Style) -> str:
    if style == "italic":
        return line.replace(word, f"*{word}*")
    elif style == "bold":
        return line.replace(word, f"__{word}__")

    position = line.find(word)
    output = line + "\n"
    output += " " * position
    output += "-" * len(word)
"#,
    );

    builder.add_python_cell(
        r#"print(with_style("ty is a fast type checker for Python.", "fast", "underlined"))
"#,
    );

    builder.open(&mut server);

    let cell1_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>();
    let cell2_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>();
    let cell3_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>();

    assert_json_snapshot!([cell1_diagnostics, cell2_diagnostics, cell3_diagnostics]);

    Ok(())
}

#[test]
fn diagnostic_end_of_file() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("test.ipynb");
    builder.add_python_cell(
        r#"from typing import Literal

type Style = Literal["italic", "bold", "underline"]"#,
    );

    builder.add_python_cell(
        r#"def with_style(line: str, word, style: Style) -> str:
    if style == "italic":
        return line.replace(word, f"*{word}*")
    elif style == "bold":
        return line.replace(word, f"__{word}__")

    position = line.find(word)
    output = line + "\n"
    output += " " * position
    output += "-" * len(word)
"#,
    );

    let cell_3 = builder.add_python_cell(
        r#"with_style("test", "word", "underline")

IOError"#,
    );

    let notebook_url = builder.open(&mut server);

    server.collect_publish_diagnostic_notifications(3);

    server.send_notification::<lsp_types::notification::DidChangeNotebookDocument>(
        lsp_types::DidChangeNotebookDocumentParams {
            notebook_document: lsp_types::VersionedNotebookDocumentIdentifier {
                version: 0,
                uri: notebook_url,
            },
            change: lsp_types::NotebookDocumentChangeEvent {
                metadata: None,
                cells: Some(lsp_types::NotebookDocumentCellChange {
                    structure: None,
                    data: None,
                    text_content: Some(vec![lsp_types::NotebookDocumentChangeTextContent {
                        document: lsp_types::VersionedTextDocumentIdentifier {
                            uri: cell_3,
                            version: 0,
                        },

                        changes: {
                            vec![lsp_types::TextDocumentContentChangeEvent {
                                range: Some(Range::new(Position::new(0, 16), Position::new(0, 17))),
                                range_length: Some(1),
                                text: String::new(),
                            }]
                        },
                    }]),
                }),
            },
        },
    );

    let diagnostics = server.collect_publish_diagnostic_notifications(3);
    assert_json_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn semantic_tokens() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");
    let first_cell = builder.add_python_cell(
        r#"from typing import Literal

type Style = Literal["italic", "bold", "underline"]"#,
    );

    let second_cell = builder.add_python_cell(
        r#"def with_style(line: str, word, style: Style) -> str:
    if style == "italic":
        return line.replace(word, f"*{word}*")
    elif style == "bold":
        return line.replace(word, f"__{word}__")

    position = line.find(word)
    output = line + "\n"
    output += " " * position
    output += "-" * len(word)
"#,
    );

    let third_cell = builder.add_python_cell(
        r#"print(with_style("ty is a fast type checker for Python.", "fast", "underlined"))
"#,
    );

    builder.open(&mut server);

    let cell1_tokens = semantic_tokens_full_for_cell(&mut server, &first_cell);
    let cell2_tokens = semantic_tokens_full_for_cell(&mut server, &second_cell);
    let cell3_tokens = semantic_tokens_full_for_cell(&mut server, &third_cell);

    assert_json_snapshot!([cell1_tokens, cell2_tokens, cell3_tokens]);

    server.collect_publish_diagnostic_notifications(3);

    Ok(())
}

#[test]
fn swap_cells() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");

    let first_cell = builder.add_python_cell(
        r#"b = a
"#,
    );

    let second_cell = builder.add_python_cell(r#"a = 10"#);

    builder.add_python_cell(r#"c = b"#);

    let notebook = builder.open(&mut server);

    let diagnostics = server.collect_publish_diagnostic_notifications(3);
    assert_json_snapshot!(diagnostics, @r###"
    {
      "vscode-notebook-cell://src/test.ipynb#0": [
        {
          "range": {
            "start": {
              "line": 0,
              "character": 4
            },
            "end": {
              "line": 0,
              "character": 5
            }
          },
          "severity": 1,
          "code": "unresolved-reference",
          "codeDescription": {
            "href": "https://ty.dev/rules#unresolved-reference"
          },
          "source": "ty",
          "message": "Name `a` used when not defined"
        }
      ],
      "vscode-notebook-cell://src/test.ipynb#1": [],
      "vscode-notebook-cell://src/test.ipynb#2": []
    }
    "###);

    // Re-order the cells from `b`, `a`, `c` to `a`, `b`, `c` (swapping cell 1 and 2)
    server.send_notification::<lsp_types::notification::DidChangeNotebookDocument>(
        lsp_types::DidChangeNotebookDocumentParams {
            notebook_document: lsp_types::VersionedNotebookDocumentIdentifier {
                version: 1,
                uri: notebook,
            },
            change: lsp_types::NotebookDocumentChangeEvent {
                metadata: None,
                cells: Some(lsp_types::NotebookDocumentCellChange {
                    structure: Some(lsp_types::NotebookDocumentCellChangeStructure {
                        array: lsp_types::NotebookCellArrayChange {
                            start: 0,
                            delete_count: 2,
                            cells: Some(vec![
                                lsp_types::NotebookCell {
                                    kind: NotebookCellKind::Code,
                                    document: second_cell,
                                    metadata: None,
                                    execution_summary: None,
                                },
                                lsp_types::NotebookCell {
                                    kind: NotebookCellKind::Code,
                                    document: first_cell,
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
            },
        },
    );

    let diagnostics = server.collect_publish_diagnostic_notifications(3);

    assert_json_snapshot!(diagnostics, @r###"
    {
      "vscode-notebook-cell://src/test.ipynb#0": [],
      "vscode-notebook-cell://src/test.ipynb#1": [],
      "vscode-notebook-cell://src/test.ipynb#2": []
    }
    "###);

    Ok(())
}

#[test]
fn auto_import() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_auto_import(true)),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");

    builder.add_python_cell(
        r#"from typing import TYPE_CHECKING
"#,
    );

    let second_cell = builder.add_python_cell(
        r#"# leading comment
b: Litera
"#,
    );

    builder.open(&mut server);

    server.collect_publish_diagnostic_notifications(2);

    let completions = literal_completions(&mut server, &second_cell, Position::new(1, 9));

    insta::with_settings!({
        filters => FILTERS.iter().copied(),
    }, {
        assert_json_snapshot!(completions);
    });

    Ok(())
}

#[test]
fn auto_import_same_cell() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_auto_import(true)),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");

    let first_cell = builder.add_python_cell(
        r#"from typing import TYPE_CHECKING
b: Litera
"#,
    );

    builder.open(&mut server);

    server.collect_publish_diagnostic_notifications(1);

    let completions = literal_completions(&mut server, &first_cell, Position::new(1, 9));

    insta::with_settings!({
        filters => FILTERS.iter().copied(),
    }, {
        assert_json_snapshot!(completions);
    });

    Ok(())
}

#[test]
fn auto_import_from_future() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_auto_import(true)),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");

    builder.add_python_cell(r#"from typing import TYPE_CHECKING"#);

    let second_cell = builder.add_python_cell(
        r#"from __future__ import annotations
b: Litera
"#,
    );

    builder.open(&mut server);

    server.collect_publish_diagnostic_notifications(2);

    let completions = literal_completions(&mut server, &second_cell, Position::new(1, 9));

    insta::with_settings!({
        filters => FILTERS.iter().copied(),
    }, {
        assert_json_snapshot!(completions);
    });

    Ok(())
}

#[test]
fn auto_import_docstring() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_auto_import(true)),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");

    builder.add_python_cell(
        r#"from typing import TYPE_CHECKING
"#,
    );

    let second_cell = builder.add_python_cell(
        r#""""A cell level docstring"""
b: Litera
"#,
    );

    builder.open(&mut server);

    server.collect_publish_diagnostic_notifications(2);

    let completions = literal_completions(&mut server, &second_cell, Position::new(1, 9));

    insta::with_settings!({
        filters => FILTERS.iter().copied(),
    }, {
        assert_json_snapshot!(completions);
    });

    Ok(())
}

#[test]
fn invalid_syntax_with_syntax_errors_disabled() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_show_syntax_errors(false)),
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    server.initialization_result().unwrap();

    let mut builder = NotebookBuilder::virtual_file("src/test.ipynb");

    builder.add_python_cell(
        r#"def foo(
"#,
    );

    builder.add_python_cell(
        r#"x = 1 +
"#,
    );

    builder.open(&mut server);

    let diagnostics = server.collect_publish_diagnostic_notifications(2);

    assert_json_snapshot!(diagnostics, @r###"
    {
      "vscode-notebook-cell://src/test.ipynb#0": [],
      "vscode-notebook-cell://src/test.ipynb#1": []
    }
    "###);

    Ok(())
}

fn semantic_tokens_full_for_cell(
    server: &mut TestServer,
    cell_uri: &lsp_types::Url,
) -> Option<lsp_types::SemanticTokensResult> {
    let cell1_tokens_req_id = server.send_request::<lsp_types::request::SemanticTokensFullRequest>(
        lsp_types::SemanticTokensParams {
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: cell_uri.clone(),
            },
        },
    );

    server.await_response::<lsp_types::request::SemanticTokensFullRequest>(&cell1_tokens_req_id)
}

#[derive(Debug)]
pub(crate) struct NotebookBuilder {
    notebook_url: lsp_types::Url,
    // The cells: (cell_metadata, content, language_id)
    cells: Vec<(lsp_types::NotebookCell, String, String)>,
}

impl NotebookBuilder {
    pub(crate) fn virtual_file(name: &str) -> Self {
        let url: lsp_types::Url = format!("vs-code:/{name}").parse().unwrap();
        Self {
            notebook_url: url,
            cells: Vec::new(),
        }
    }

    pub(crate) fn add_python_cell(&mut self, content: &str) -> lsp_types::Url {
        let index = self.cells.len();
        let id = format!(
            "vscode-notebook-cell:/{}#{}",
            self.notebook_url.path(),
            index
        );

        let url: lsp_types::Url = id.parse().unwrap();

        self.cells.push((
            lsp_types::NotebookCell {
                kind: NotebookCellKind::Code,
                document: url.clone(),
                metadata: None,
                execution_summary: None,
            },
            content.to_string(),
            "python".to_string(),
        ));

        url
    }

    pub(crate) fn open(self, server: &mut TestServer) -> lsp_types::Url {
        server.send_notification::<lsp_types::notification::DidOpenNotebookDocument>(
            lsp_types::DidOpenNotebookDocumentParams {
                notebook_document: lsp_types::NotebookDocument {
                    uri: self.notebook_url.clone(),
                    notebook_type: "jupyter-notebook".to_string(),
                    version: 0,
                    metadata: None,
                    cells: self.cells.iter().map(|(cell, _, _)| cell.clone()).collect(),
                },
                cell_text_documents: self
                    .cells
                    .iter()
                    .map(|(cell, content, language_id)| lsp_types::TextDocumentItem {
                        uri: cell.document.clone(),
                        language_id: language_id.clone(),
                        version: 0,
                        text: content.clone(),
                    })
                    .collect(),
            },
        );

        self.notebook_url
    }
}

fn literal_completions(
    server: &mut TestServer,
    cell: &lsp_types::Url,
    position: Position,
) -> Vec<lsp_types::CompletionItem> {
    let mut items = server.completion_request(cell, position);
    // There are a ton of imports we don't care about in here...
    // The import bit is that an edit is always restricted to the current cell. That means,
    // we can't add `Literal` to the `from typing import TYPE_CHECKING` import in cell 1
    items.retain(|item| item.label.starts_with("Litera"));
    items
}
