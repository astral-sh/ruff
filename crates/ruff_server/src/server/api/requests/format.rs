use anyhow::Context;
use lsp_types::{self as types, DocumentFormattingRequest};
use types::TextEdit;

use ruff_source_file::LineIndex;

use crate::edit::{Replacement, ToRangeExt};
use crate::fix::Fixes;
use crate::format::FormatResult;
use crate::resolve::is_document_excluded_for_formatting;
use crate::server::Result;
use crate::server::api::LSPResult;
use crate::session::{Client, DocumentQuery, DocumentSnapshot};
use crate::{PositionEncoding, TextDocument};

pub(crate) struct Format;

impl super::RequestHandler for Format {
    type RequestType = DocumentFormattingRequest;
}

impl super::BackgroundDocumentRequestHandler for Format {
    super::define_document_url!(params: &types::DocumentFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        client: &Client,
        _params: types::DocumentFormattingParams,
    ) -> Result<super::FormatResponse> {
        format_document(&snapshot, client)
    }
}

/// Formats either a full text document or each individual cell in a single notebook document.
pub(super) fn format_full_document(snapshot: &DocumentSnapshot, client: &Client) -> Result<Fixes> {
    let mut fixes = Fixes::default();
    let query = snapshot.query();
    let backend = snapshot
        .client_settings()
        .editor_settings()
        .format_backend();

    match snapshot.query() {
        DocumentQuery::Notebook { notebook, .. } => {
            for (url, text_document) in notebook
                .urls()
                .map(|url| (url.clone(), notebook.cell_document_by_uri(url).unwrap()))
            {
                if let Some(changes) = format_text_document(
                    text_document,
                    query,
                    snapshot.encoding(),
                    true,
                    backend,
                    client,
                )? {
                    fixes.insert(url, changes);
                }
            }
        }
        DocumentQuery::Text { document, .. } => {
            if let Some(changes) =
                format_text_document(document, query, snapshot.encoding(), false, backend, client)?
            {
                fixes.insert(snapshot.query().make_key().into_url(), changes);
            }
        }
    }

    Ok(fixes)
}

/// Formats either a full text document or an specific notebook cell. If the query within the snapshot is a notebook document
/// with no selected cell, this will throw an error.
pub(super) fn format_document(
    snapshot: &DocumentSnapshot,
    client: &Client,
) -> Result<super::FormatResponse> {
    let text_document = snapshot
        .query()
        .as_single_document()
        .context("Failed to get text document for the format request")
        .unwrap();
    let query = snapshot.query();
    let backend = snapshot
        .client_settings()
        .editor_settings()
        .format_backend();
    format_text_document(
        text_document,
        query,
        snapshot.encoding(),
        query.as_notebook().is_some(),
        backend,
        client,
    )
}

fn format_text_document(
    text_document: &TextDocument,
    query: &DocumentQuery,
    encoding: PositionEncoding,
    is_notebook: bool,
    backend: crate::format::FormatBackend,
    client: &Client,
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

    let source = text_document.contents();
    let formatted = crate::format::format(
        text_document,
        source_type,
        &settings.formatter,
        &file_path,
        backend,
    )
    .with_failure_code(lsp_server::ErrorCode::InternalError)?;
    let mut formatted = match formatted {
        FormatResult::Formatted(formatted) => formatted,
        FormatResult::Unchanged => return Ok(None),
        FormatResult::PreviewOnly { file_format } => {
            client.show_warning_message(
                format_args!(
                    "{file_format} formatting is available only in preview mode. Enable `format.preview = true` in your Ruff configuration."
                ),
            );
            return Ok(None);
        }
    };

    // special case - avoid adding a newline to a notebook cell if it didn't already exist
    if is_notebook {
        let mut trimmed = formatted.as_str();
        if !source.ends_with("\r\n") {
            trimmed = trimmed.trim_end_matches("\r\n");
        }
        if !source.ends_with('\n') {
            trimmed = trimmed.trim_end_matches('\n');
        }
        if !source.ends_with('\r') {
            trimmed = trimmed.trim_end_matches('\r');
        }
        formatted = trimmed.to_string();
    }

    let formatted_index: LineIndex = LineIndex::from_source_text(&formatted);

    let unformatted_index = text_document.index();

    let Replacement {
        source_range,
        modified_range: formatted_range,
    } = Replacement::between(
        source,
        unformatted_index.line_starts(),
        &formatted,
        formatted_index.line_starts(),
    );

    Ok(Some(vec![TextEdit {
        range: source_range.to_range(source, unformatted_index, encoding),
        new_text: formatted[formatted_range].to_owned(),
    }]))
}
