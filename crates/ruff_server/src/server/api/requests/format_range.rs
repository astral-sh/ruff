use lsp_types::{self as types, request as req, Range};

use crate::edit::{RangeExt, ToRangeExt};
use crate::resolve::is_document_excluded;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::{DocumentQuery, DocumentSnapshot};
use crate::{PositionEncoding, TextDocument};

pub(crate) struct FormatRange;

impl super::RequestHandler for FormatRange {
    type RequestType = req::RangeFormatting;
}

impl super::BackgroundDocumentRequestHandler for FormatRange {
    super::define_document_url!(params: &types::DocumentRangeFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
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
        .expect("format should only be called on text documents or notebook cells");
    let query = snapshot.query();
    format_text_document_range(text_document, range, query, snapshot.encoding())
}

/// Formats the specified [`Range`] in the [`TextDocument`].
fn format_text_document_range(
    text_document: &TextDocument,
    range: Range,
    query: &DocumentQuery,
    encoding: PositionEncoding,
) -> Result<super::FormatResponse> {
    let file_resolver_settings = query.settings().file_resolver();
    let formatter_settings = query.settings().formatter();

    // If the document is excluded, return early.
    if let Some(file_path) = query.file_path() {
        if is_document_excluded(
            &file_path,
            file_resolver_settings,
            None,
            Some(formatter_settings),
            text_document.language_id(),
        ) {
            return Ok(None);
        }
    }

    let text = text_document.contents();
    let index = text_document.index();
    let range = range.to_text_range(text, index, encoding);
    let formatted_range = crate::format::format_range(
        text_document,
        query.source_type(),
        formatter_settings,
        range,
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
