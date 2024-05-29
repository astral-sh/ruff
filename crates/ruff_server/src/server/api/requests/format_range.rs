use std::path::Path;

use lsp_types::{self as types, request as req, Range};

use ruff_python_ast::PySourceType;
use ruff_workspace::resolver::match_any_exclusion;
use ruff_workspace::{FileResolverSettings, FormatterSettings};

use crate::edit::{RangeExt, ToRangeExt};
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
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
    format_text_document_range(
        text_document,
        range,
        query.source_type(),
        query.file_path(),
        query.settings().file_resolver(),
        query.settings().formatter(),
        snapshot.encoding(),
    )
}

/// Formats the specified [`Range`] in the [`TextDocument`].
fn format_text_document_range(
    text_document: &TextDocument,
    range: Range,
    source_type: PySourceType,
    file_path: &Path,
    file_resolver_settings: &FileResolverSettings,
    formatter_settings: &FormatterSettings,
    encoding: PositionEncoding,
) -> Result<super::FormatResponse> {
    // If the document is excluded, return early.
    if let Some(exclusion) = match_any_exclusion(
        file_path,
        &file_resolver_settings.exclude,
        &file_resolver_settings.extend_exclude,
        None,
        Some(&formatter_settings.exclude),
    ) {
        tracing::debug!("Ignored path via `{}`: {}", exclusion, file_path.display());
        return Ok(None);
    }

    let text = text_document.contents();
    let index = text_document.index();
    let range = range.to_text_range(text, index, encoding);
    let formatted_range =
        crate::format::format_range(text_document, source_type, formatter_settings, range)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    Ok(Some(vec![types::TextEdit {
        range: formatted_range
            .source_range()
            .to_range(text, index, encoding),
        new_text: formatted_range.into_code(),
    }]))
}
