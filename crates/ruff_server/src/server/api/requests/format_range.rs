use anyhow::Context;
use lsp_types::{self as types, Range, request as req};

use crate::edit::{RangeExt, ToRangeExt};
use crate::resolve::is_document_excluded_for_formatting;
use crate::server::Result;
use crate::server::api::LSPResult;
use crate::session::{Client, DocumentQuery, DocumentSnapshot};
use crate::{PositionEncoding, TextDocument};

pub(crate) struct FormatRange;

impl super::RequestHandler for FormatRange {
    type RequestType = req::RangeFormatting;
}

impl super::BackgroundDocumentRequestHandler for FormatRange {
    super::define_document_url!(params: &types::DocumentRangeFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: types::DocumentRangeFormattingParams,
    ) -> Result<super::FormatResponse> {
        format_document_range(&snapshot, params.range)
    }
}

/// Formats the specified [`Range`] in the [`DocumentSnapshot`].
fn format_document_range(
    snapshot: &DocumentSnapshot,
    range: Range,
) -> Result<super::FormatResponse> {
    let text_document = snapshot
        .query()
        .as_single_document()
        .context("Failed to get text document for the format range request")
        .unwrap();
    let query = snapshot.query();
    let backend = snapshot
        .client_settings()
        .editor_settings()
        .format_backend();
    format_text_document_range(text_document, range, query, snapshot.encoding(), backend)
}

/// Formats the specified [`Range`] in the [`TextDocument`].
fn format_text_document_range(
    text_document: &TextDocument,
    range: Range,
    query: &DocumentQuery,
    encoding: PositionEncoding,
    backend: crate::format::FormatBackend,
) -> Result<super::FormatResponse> {
    let settings = query.settings();
    let file_path = query.virtual_file_path();
    let source_type = query.source_type_for_format();

    // If the document is excluded, return early.
    if is_document_excluded_for_formatting(
        &file_path,
        &settings.file_resolver,
        &settings.formatter,
        text_document.language_id(),
    ) {
        return Ok(None);
    }

    let text = text_document.contents();
    let index = text_document.index();
    let range = range.to_text_range(text, index, encoding);
    let formatted_range = crate::format::format_range(
        text_document,
        source_type,
        &settings.formatter,
        range,
        &file_path,
        backend,
    )
    .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    Ok(formatted_range.map(|formatted_range| {
        vec![types::TextEdit {
            range: formatted_range
                .source_range()
                .to_range(text, index, encoding),
            new_text: formatted_range.into_code(),
        }]
    }))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use lsp_types::{ClientCapabilities, Position, Range, Url};

    use crate::server::api::requests::format_range::format_document_range;
    use crate::session::{Client, GlobalOptions};
    use crate::{PositionEncoding, TextDocument, Workspace, Workspaces};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("ruff-server-{name}-{nanos}"))
    }

    #[test]
    fn range_format_custom_extension_mapped_to_markdown_is_unsupported() {
        let workspace_dir = unique_temp_dir("custom-markdown-range");
        fs::create_dir_all(&workspace_dir).expect("create temp workspace");
        fs::write(
            workspace_dir.join("pyproject.toml"),
            r#"[tool.ruff]
extension = { thing = "markdown" }

[tool.ruff.format]
preview = true
"#,
        )
        .expect("write pyproject");

        let file_path = workspace_dir.join("test.thing");
        let content = "# title\n\n```python\nx='hi'\n```\n";
        fs::write(&file_path, content).expect("write document");

        let (main_loop_sender, _) = crossbeam::channel::unbounded();
        let (client_sender, _) = crossbeam::channel::unbounded();
        let client = Client::new(main_loop_sender, client_sender);

        let workspace_url = Url::from_file_path(&workspace_dir).expect("workspace url");
        let global = GlobalOptions::default().into_settings(client.clone());

        let mut session = crate::Session::new(
            &ClientCapabilities::default(),
            PositionEncoding::UTF16,
            global,
            &Workspaces::new(vec![
                Workspace::new(workspace_url).with_options(crate::ClientOptions::default()),
            ]),
            &client,
        )
        .expect("create session");

        let file_url = Url::from_file_path(&file_path).expect("file url");
        let document = TextDocument::new(content.to_string(), 0).with_language_id("markdown");
        session.open_text_document(file_url.clone(), document);

        let snapshot = session.take_snapshot(file_url).expect("snapshot");
        let result = format_document_range(
            &snapshot,
            Range {
                start: Position {
                    line: 2,
                    character: 0,
                },
                end: Position {
                    line: 3,
                    character: 6,
                },
            },
        )
        .expect("range format request should succeed");

        assert!(
            result.is_none(),
            "expected no range formatting edits for markdown-mapped extension"
        );

        fs::remove_dir_all(&workspace_dir).ok();
    }
}
