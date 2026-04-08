use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::request::{DocumentDiagnosticRequest, Formatting, RangeFormatting};
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReportResult, DocumentFormattingParams,
    DocumentRangeFormattingParams, PartialResultParams, Position, Range, TextDocumentIdentifier,
    WorkDoneProgressParams,
};

use crate::TestServerBuilder;

const CUSTOM_EXTENSION_CONFIG: &str = r#"[tool.ruff]
preview = true
extension = { thing = "markdown" }

[tool.ruff.format]
preview = true
"#;

const CUSTOM_EXTENSION_MARKDOWN: &str = "# title\n\n```python\nx='hi'\n```\n";

#[test]
fn format_custom_extension_mapped_to_markdown() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", CUSTOM_EXTENSION_CONFIG)?
        .build();

    server.open_text_document_with_language_id(
        "test.thing",
        "markdown",
        CUSTOM_EXTENSION_MARKDOWN,
        1,
    );

    let request = DocumentFormattingParams {
        text_document: TextDocumentIdentifier {
            uri: server.file_uri("test.thing"),
        },
        options: lsp_types::FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            ..Default::default()
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let id = server.send_request::<Formatting>(request);
    let result = server.await_response::<Formatting>(&id);

    assert_json_snapshot!(
        result,
        @r#"
    [
      {
        "range": {
          "start": {
            "line": 3,
            "character": 0
          },
          "end": {
            "line": 4,
            "character": 0
          }
        },
        "newText": "x = \"hi\"\n"
      }
    ]
    "#
    );

    Ok(())
}

#[test]
fn range_format_custom_extension_mapped_to_markdown_is_unsupported() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", CUSTOM_EXTENSION_CONFIG)?
        .build();

    server.open_text_document_with_language_id(
        "test.thing",
        "markdown",
        CUSTOM_EXTENSION_MARKDOWN,
        1,
    );

    let request = DocumentRangeFormattingParams {
        text_document: TextDocumentIdentifier {
            uri: server.file_uri("test.thing"),
        },
        range: Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 6,
            },
        },
        options: lsp_types::FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            ..Default::default()
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let id = server.send_request::<RangeFormatting>(request);
    let result = server.await_response::<RangeFormatting>(&id);

    assert_json_snapshot!(result, @"null");

    Ok(())
}

#[test]
fn lint_custom_extension_mapped_to_markdown_emits_no_diagnostics() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", CUSTOM_EXTENSION_CONFIG)?
        .enable_pull_diagnostics(true)
        .build();

    server.open_text_document_with_language_id(
        "test.thing",
        "markdown",
        CUSTOM_EXTENSION_MARKDOWN,
        1,
    );

    let request = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier {
            uri: server.file_uri("test.thing"),
        },
        identifier: Some("Ruff".to_string()),
        previous_result_id: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let id = server.send_request::<DocumentDiagnosticRequest>(request);
    let result: DocumentDiagnosticReportResult =
        server.await_response::<DocumentDiagnosticRequest>(&id);

    assert_json_snapshot!(
        result,
        @r#"
    {
      "kind": "full",
      "items": []
    }
    "#
    );

    Ok(())
}
