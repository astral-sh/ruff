use crate::edit::text_range_to_range;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::SessionSnapshot;
use lsp_types::{self as types, request as req};
use ruff_text_size::{TextRange, TextSize};
use types::TextEdit;

pub(crate) struct Format;

impl super::Request for Format {
    type RequestType = req::Formatting;
}

impl super::BackgroundRequest for Format {
    super::define_document_url!(params: &types::DocumentFormattingParams);
    fn run_with_snapshot(
        snapshot: SessionSnapshot,
        _notifier: Notifier,
        _params: types::DocumentFormattingParams,
    ) -> Result<super::FormatResponse> {
        let code = crate::format::format(snapshot.document(), &snapshot.configuration().formatter)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        let doc_size = TextSize::of(snapshot.document().contents());
        // TODO(jane): Can we try breaking this up into individual text edits instead of replacing the whole document?
        Ok(Some(vec![TextEdit {
            range: text_range_to_range(
                TextRange::up_to(doc_size),
                snapshot.document(),
                snapshot.encoding(),
            ),
            new_text: code,
        }]))
    }
}
