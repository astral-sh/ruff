use std::borrow::Cow;

use lsp_types::{self as types, Url, request as req};
use ty_project::ProjectDatabase;
use types::{CodeActionKind, CodeActionOrCommand};

use crate::DIAGNOSTIC_NAME;
use crate::server::Result;
use crate::server::api::diagnostics::DiagnosticData;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RetriableRequestHandler};
use crate::server::api::{LSPResult, RequestHandler};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct CodeActionRequestHandler;

impl RequestHandler for CodeActionRequestHandler {
    type RequestType = req::CodeActionRequest;
}

impl BackgroundDocumentRequestHandler for CodeActionRequestHandler {
    fn document_url(params: &types::CodeActionParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        _db: &ProjectDatabase,
        _snapshot: &DocumentSnapshot,
        _client: &Client,
        params: types::CodeActionParams,
    ) -> Result<Option<types::CodeActionResponse>> {
        let diagnostics = params.context.diagnostics;

        let diagnostic_actions = diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.source.as_deref() == Some(DIAGNOSTIC_NAME)
                    && range_intersect(&diagnostic.range, &params.range)
            })
            .map(|mut diagnostic| {
                let Some(data) = diagnostic.data.take() else {
                    return Ok(None);
                };
                let data: DiagnosticData = serde_json::from_value(data).map_err(|err| {
                    anyhow::anyhow!("Failed to deserialize diagnostic data: {err}")
                })?;

                Ok(Some(CodeActionOrCommand::CodeAction(
                    lsp_types::CodeAction {
                        title: data.fix_title,
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diagnostic]),
                        edit: Some(lsp_types::WorkspaceEdit {
                            changes: Some(data.edits),
                            document_changes: None,
                            change_annotations: None,
                        }),
                        is_preferred: Some(true),
                        command: None,
                        disabled: None,
                        data: None,
                    },
                )))
            })
            .filter_map(crate::Result::transpose)
            .collect::<crate::Result<Vec<_>>>()
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        if diagnostic_actions.is_empty() {
            return Ok(None);
        }

        Ok(Some(diagnostic_actions))
    }
}

fn range_intersect(range: &lsp_types::Range, other: &lsp_types::Range) -> bool {
    let start = range.start.max(other.start);
    let end = range.end.min(other.end);
    end >= start
}

impl RetriableRequestHandler for CodeActionRequestHandler {}
