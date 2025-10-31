use crate::{TestServer, TestServerBuilder};
use insta::assert_debug_snapshot;
use lsp_types::{CompletionTriggerKind, NotebookCellKind, Position, Range};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

#[test]
fn publish_diagnostics_open() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .build()?
        .wait_until_workspaces_are_initialized()?;

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
        server.await_notification::<lsp_types::notification::PublishDiagnostics>()?;
    let cell2_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>()?;
    let cell3_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>()?;

    assert_debug_snapshot!([cell1_diagnostics, cell2_diagnostics, cell3_diagnostics]);

    Ok(())
}

#[test]
fn diagnostic_end_of_file() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .build()?
        .wait_until_workspaces_are_initialized()?;

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

    server.ignore_notifications::<lsp_types::notification::PublishDiagnostics>(3)?;

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
                            uri: cell_3.clone(),
                            version: 0,
                        },

                        changes: {
                            vec![lsp_types::TextDocumentContentChangeEvent {
                                range: Some(Range::new(Position::new(0, 16), Position::new(0, 17))),
                                range_length: Some(1),
                                text: "".to_string(),
                            }]
                        },
                    }]),
                }),
            },
        },
    );

    let cell1_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>()?;
    let cell2_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>()?;
    let cell3_diagnostics =
        server.await_notification::<lsp_types::notification::PublishDiagnostics>()?;

    assert_debug_snapshot!([cell1_diagnostics, cell2_diagnostics, cell3_diagnostics]);

    Ok(())
}

#[test]
fn semantic_tokens() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .build()?
        .wait_until_workspaces_are_initialized()?;

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

    let cell1_tokens = semantic_tokens_full_for_cell(&mut server, &first_cell)?;
    let cell2_tokens = semantic_tokens_full_for_cell(&mut server, &second_cell)?;
    let cell3_tokens = semantic_tokens_full_for_cell(&mut server, &third_cell)?;

    assert_debug_snapshot!([cell1_tokens, cell2_tokens, cell3_tokens]);

    server.ignore_notifications::<lsp_types::notification::PublishDiagnostics>(3)?;

    Ok(())
}

#[test]
fn auto_import() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_experimental_auto_import(true)),
        )?
        .build()?
        .wait_until_workspaces_are_initialized()?;

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

    server.ignore_notifications::<lsp_types::notification::PublishDiagnostics>(2)?;

    let completions_id =
        server.send_request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier {
                    uri: second_cell.clone(),
                },
                position: Position::new(1, 9),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: Some(lsp_types::CompletionContext {
                trigger_kind: CompletionTriggerKind::TRIGGER_FOR_INCOMPLETE_COMPLETIONS,
                trigger_character: None,
            }),
        });

    // There are a ton of imports we don't care about in here...
    // The import bit is that an edit is always restricted to the current cell. That means,
    // we can't add `Literal` to the `from typing import TYPE_CHECKING` import in cell 1
    let completions = server.await_response::<lsp_types::request::Completion>(&completions_id)?;

    assert_debug_snapshot!(completions);

    Ok(())
}

#[test]
fn auto_import_docstring() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(
            SystemPath::new("src"),
            Some(ClientOptions::default().with_experimental_auto_import(true)),
        )?
        .build()?
        .wait_until_workspaces_are_initialized()?;

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

    server.ignore_notifications::<lsp_types::notification::PublishDiagnostics>(2)?;

    let completions_id =
        server.send_request::<lsp_types::request::Completion>(lsp_types::CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier {
                    uri: second_cell.clone(),
                },
                position: Position::new(1, 9),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: Some(lsp_types::CompletionContext {
                trigger_kind: CompletionTriggerKind::TRIGGER_FOR_INCOMPLETE_COMPLETIONS,
                trigger_character: None,
            }),
        });

    // There are a ton of imports we don't care about in here...
    // The import bit is that an edit is always restricted to the current cell. That means,
    // we can't add `Literal` to the `from typing import TYPE_CHECKING` import in cell 1
    let completions = server.await_response::<lsp_types::request::Completion>(&completions_id)?;

    assert_debug_snapshot!(completions);

    Ok(())
}

fn semantic_tokens_full_for_cell(
    server: &mut TestServer,
    cell_uri: &lsp_types::Url,
) -> crate::Result<Option<lsp_types::SemanticTokensResult>> {
    let cell1_tokens_req_id = server.send_request::<lsp_types::request::SemanticTokensFullRequest>(
        lsp_types::SemanticTokensParams {
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
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
        let url: lsp_types::Url = format!("vs-code:/{}", name).parse().unwrap();
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

        self.notebook_url.clone()
    }
}
