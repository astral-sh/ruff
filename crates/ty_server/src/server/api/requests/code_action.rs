use std::borrow::Cow;
use std::collections::HashMap;

use lsp_types::{self as types, NumberOrString, TextEdit, Url, request as req};
use ruff_db::files::File;
use ruff_diagnostics::Edit;
use ruff_text_size::Ranged;
use ty_ide::code_actions;
use ty_project::ProjectDatabase;
use types::{CodeActionKind, CodeActionOrCommand};

use crate::db::Db;
use crate::document::{RangeExt, ToRangeExt};
use crate::server::Result;
use crate::server::api::RequestHandler;
use crate::server::api::diagnostics::DiagnosticData;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RetriableRequestHandler};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use crate::{DIAGNOSTIC_NAME, PositionEncoding};

pub(crate) struct CodeActionRequestHandler;

impl RequestHandler for CodeActionRequestHandler {
    type RequestType = req::CodeActionRequest;
}

impl BackgroundDocumentRequestHandler for CodeActionRequestHandler {
    fn document_url(params: &types::CodeActionParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: types::CodeActionParams,
    ) -> Result<Option<types::CodeActionResponse>> {
        let diagnostics = params.context.diagnostics;

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };
        let mut actions = Vec::new();

        for mut diagnostic in diagnostics.into_iter().filter(|diagnostic| {
            diagnostic.source.as_deref() == Some(DIAGNOSTIC_NAME)
                && range_intersect(&diagnostic.range, &params.range)
        }) {
            // If the diagnostic includes fixes, offer those up as options.
            if let Some(data) = diagnostic.data.take() {
                let data: DiagnosticData = match serde_json::from_value(data) {
                    Ok(data) => data,
                    Err(err) => {
                        tracing::warn!("Failed to deserialize diagnostic data: {err}");
                        continue;
                    }
                };

                actions.push(CodeActionOrCommand::CodeAction(lsp_types::CodeAction {
                    title: data.fix_title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diagnostic.clone()]),
                    edit: Some(lsp_types::WorkspaceEdit {
                        changes: Some(data.edits),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    is_preferred: Some(true),
                    command: None,
                    disabled: None,
                    data: None,
                }));
            }

            // Try to find other applicable actions.
            //
            // This is only for actions that are messy to compute at the time of the diagnostic.
            // For instance, suggesting imports requires finding symbols for the entire project,
            // which is dubious when you're in the middle of resolving symbols.
            let url = snapshot.url();
            let encoding = snapshot.encoding();
            if let Some(NumberOrString::String(diagnostic_id)) = &diagnostic.code
                && let Some(range) = diagnostic.range.to_text_range(db, file, url, encoding)
            {
                for action in code_actions(db, file, range, diagnostic_id) {
                    actions.push(CodeActionOrCommand::CodeAction(lsp_types::CodeAction {
                        title: action.title,
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diagnostic.clone()]),
                        edit: Some(lsp_types::WorkspaceEdit {
                            changes: to_lsp_edits(db, file, encoding, action.edits),
                            document_changes: None,
                            change_annotations: None,
                        }),
                        is_preferred: Some(action.preferred),
                        command: None,
                        disabled: None,
                        data: None,
                    }));
                }
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }
}

fn to_lsp_edits(
    db: &dyn Db,
    file: File,
    encoding: PositionEncoding,
    edits: Vec<Edit>,
) -> Option<HashMap<Url, Vec<TextEdit>>> {
    let mut lsp_edits: HashMap<Url, Vec<lsp_types::TextEdit>> = HashMap::new();

    for edit in edits {
        let location = edit
            .range()
            .to_lsp_range(db, file, encoding)?
            .to_location()?;

        lsp_edits
            .entry(location.uri)
            .or_default()
            .push(lsp_types::TextEdit {
                range: location.range,
                new_text: edit.content().unwrap_or_default().to_string(),
            });
    }

    Some(lsp_edits)
}

fn range_intersect(range: &lsp_types::Range, other: &lsp_types::Range) -> bool {
    let start = range.start.max(other.start);
    let end = range.end.min(other.end);
    end >= start
}

impl RetriableRequestHandler for CodeActionRequestHandler {}
