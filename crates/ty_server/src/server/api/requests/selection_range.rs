use std::borrow::Cow;

use lsp_types::request::SelectionRangeRequest;
use lsp_types::{SelectionRange as LspSelectionRange, SelectionRangeParams, Url};
use ruff_db::source::{line_index, source_text};
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

        let Some(file) = snapshot.file(db) else {
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);

        let mut results = Vec::new();

        for position in params.positions {
            let offset = position.to_text_size(&source, &line_index, snapshot.encoding());

            let ranges = selection_range(db, file, offset);
            if !ranges.is_empty() {
                // Convert ranges to nested LSP SelectionRange structure
                let mut lsp_range = None;
                for &range in &ranges {
                    lsp_range = Some(LspSelectionRange {
                        range: range.to_lsp_range(&source, &line_index, snapshot.encoding()),
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
