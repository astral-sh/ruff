use std::borrow::Cow;

use lsp_types::request::FoldingRangeRequest;
use lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams, Url};
use ty_ide::folding_ranges;
use ty_project::ProjectDatabase;

use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct FoldingRangeRequestHandler;

impl RequestHandler for FoldingRangeRequestHandler {
    type RequestType = FoldingRangeRequest;
}

impl BackgroundDocumentRequestHandler for FoldingRangeRequestHandler {
    fn document_url(params: &FoldingRangeParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        _params: FoldingRangeParams,
    ) -> crate::server::Result<Option<Vec<FoldingRange>>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let results: Vec<_> = folding_ranges(db, file)
            .into_iter()
            .filter_map(|folding_range| {
                let lsp_range = folding_range
                    .range
                    .to_lsp_range(db, file, snapshot.encoding())?;

                let kind = folding_range.kind.map(|k| match k {
                    ty_ide::FoldingRangeKind::Comment => FoldingRangeKind::Comment,
                    ty_ide::FoldingRangeKind::Imports => FoldingRangeKind::Imports,
                    ty_ide::FoldingRangeKind::Region => FoldingRangeKind::Region,
                });

                Some(FoldingRange {
                    start_line: lsp_range.local_range().start.line,
                    start_character: Some(lsp_range.local_range().start.character),
                    end_line: lsp_range.local_range().end.line,
                    end_character: Some(lsp_range.local_range().end.character),
                    kind,
                    collapsed_text: None,
                })
            })
            .collect();

        if results.is_empty() {
            Ok(None)
        } else {
            Ok(Some(results))
        }
    }
}

impl RetriableRequestHandler for FoldingRangeRequestHandler {}
