use std::borrow::Cow;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::client::Notifier;
use crate::DocumentSnapshot;
use lsp_types::request::InlayHintRequest;
use lsp_types::{InlayHintParams, Url};
use red_knot_ide::get_inlay_hints;
use red_knot_project::Db;
use red_knot_project::ProjectDatabase;
use ruff_db::source::{line_index, source_text};
use ruff_text_size::Ranged;

pub(crate) struct InlayHintRequestHandler;

impl RequestHandler for InlayHintRequestHandler {
    type RequestType = InlayHintRequest;
}

impl BackgroundDocumentRequestHandler for InlayHintRequestHandler {
    fn document_url(params: &InlayHintParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: ProjectDatabase,
        _notifier: Notifier,
        params: InlayHintParams,
    ) -> crate::server::Result<Option<Vec<lsp_types::InlayHint>>> {
        let Some(file) = snapshot.file(&db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let editor_options = db
            .project()
            .metadata(&db)
            .options()
            .editor
            .clone()
            .unwrap_or_default();

        if !editor_options.inlay_hints.unwrap_or(false) {
            return Ok(None);
        }

        let inlay_hints = get_inlay_hints(&db, file);

        let index = line_index(&db, file);
        let source = source_text(&db, file);

        let inlay_hints = inlay_hints
            .into_iter()
            .map(|hint| {
                let end = index.source_location(hint.range.range().end(), &source);

                lsp_types::InlayHint {
                    position: lsp_types::Position {
                        line: u32::try_from(end.row.to_zero_indexed())
                            .expect("row usize fits in u32"),
                        character: u32::try_from(end.column.to_zero_indexed())
                            .expect("character usize fits in u32"),
                    },
                    label: lsp_types::InlayHintLabel::String(hint.display(&db).to_string()),
                    kind: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: None,
                    data: None,
                    text_edits: None,
                }
            })
            .collect();

        Ok(Some(inlay_hints))
    }
}
