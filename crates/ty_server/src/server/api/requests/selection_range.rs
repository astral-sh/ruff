use std::borrow::Cow;

use lsp_types::request::SelectionRangeRequest;
use lsp_types::{SelectionRange as LspSelectionRange, SelectionRangeParams, Url};
use ty_ide::selection_range;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToRangeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct SelectionRangeRequestHandler;

impl RequestHandler for SelectionRangeRequestHandler {
    type RequestType = SelectionRangeRequest;
}

impl BackgroundDocumentRequestHandler for SelectionRangeRequestHandler {
    fn document_url(params: &SelectionRangeParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: SelectionRangeParams,
    ) -> crate::server::Result<Option<Vec<LspSelectionRange>>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let mut results = Vec::new();

        for position in params.positions {
            let Some(offset) = position.to_text_size(db, file, snapshot.url(), snapshot.encoding())
            else {
                continue;
            };

            let ranges = selection_range(db, file, offset);
            if !ranges.is_empty() {
                // Convert ranges to nested LSP SelectionRange structure
                let mut lsp_range = None;
                for &range in &ranges {
                    let Some(range) = range
                        .to_lsp_range(db, file, snapshot.encoding())
                        .map(|lsp_range| lsp_range.local_range())
                    else {
                        break;
                    };

                    lsp_range = Some(LspSelectionRange {
                        range,
                        parent: lsp_range.map(Box::new),
                    });
                }
                if let Some(range) = lsp_range {
                    results.push(range);
                }
            }
        }

        if results.is_empty() {
            Ok(None)
        } else {
            Ok(Some(results))
        }
    }
}

impl RetriableRequestHandler for SelectionRangeRequestHandler {}
