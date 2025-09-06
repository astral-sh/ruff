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
        query.source_type(),
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
