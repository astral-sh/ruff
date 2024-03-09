use crate::edit::{RangeExt, ToRangeExt};
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};

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
        let document = snapshot.document();
        let text = document.contents();
        let index = document.index();
        let range = params.range.to_text_range(text, index, snapshot.encoding());
        let formatted_range =
            crate::format::format_range(document, &snapshot.configuration().formatter, range)
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        Ok(Some(vec![types::TextEdit {
            range: formatted_range
                .source_range()
                .to_range(text, index, snapshot.encoding()),
            new_text: formatted_range.into_code(),
        }]))
    }
}
