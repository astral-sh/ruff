use crate::edit::{text_range, text_range_to_range};
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::SessionSnapshot;
use lsp_types::{self as types, request as req};

pub(crate) struct FormatRange;

impl super::Request for FormatRange {
    type RequestType = req::RangeFormatting;
}

impl super::BackgroundRequest for FormatRange {
    super::define_document_url!(params: &types::DocumentRangeFormattingParams);
    fn run_with_snapshot(
        snapshot: SessionSnapshot,
        _notifier: Notifier,
        params: types::DocumentRangeFormattingParams,
    ) -> Result<super::FormatResponse> {
        let document = snapshot.document();
        let range = text_range(
            params.range,
            document.contents(),
            document.index(),
            snapshot.encoding(),
        );
        let formatted_range =
            crate::format::range_format(document, &snapshot.configuration().formatter, range)
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        Ok(Some(vec![types::TextEdit {
            range: text_range_to_range(
                formatted_range.source_range(),
                document,
                snapshot.encoding(),
            ),
            new_text: formatted_range.into_code(),
        }]))
    }
}
