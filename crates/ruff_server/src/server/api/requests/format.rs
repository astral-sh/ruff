use crate::edit::{Replacement, ToRangeExt};
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};
use ruff_source_file::LineIndex;
use types::TextEdit;

pub(crate) struct Format;

impl super::RequestHandler for Format {
    type RequestType = req::Formatting;
}

impl super::BackgroundDocumentRequestHandler for Format {
    super::define_document_url!(params: &types::DocumentFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        _params: types::DocumentFormattingParams,
    ) -> Result<super::FormatResponse> {
        format_document(&snapshot)
    }
}

pub(super) fn format_document(snapshot: &DocumentSnapshot) -> Result<super::FormatResponse> {
    let doc = snapshot.document();
    let source = doc.contents();
    let formatted = crate::format::format(doc, snapshot.settings().formatter())
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;
    // fast path - if the code is the same, return early
    if formatted == source {
        return Ok(None);
    }
    let formatted_index: LineIndex = LineIndex::from_source_text(&formatted);

    let unformatted_index = doc.index();

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
        range: source_range.to_range(source, unformatted_index, snapshot.encoding()),
        new_text: formatted[formatted_range].to_owned(),
    }]))
}
