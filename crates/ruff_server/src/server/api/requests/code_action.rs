use crate::edit::ToRangeExt;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};
use ruff_text_size::Ranged;

pub(crate) struct CodeAction;

impl super::RequestHandler for CodeAction {
    type RequestType = req::CodeActionRequest;
}

impl super::BackgroundDocumentRequestHandler for CodeAction {
    super::define_document_url!(params: &types::CodeActionParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: types::CodeActionParams,
    ) -> Result<Option<types::CodeActionResponse>> {
        let document = snapshot.document();
        let url = snapshot.url();
        let encoding = snapshot.encoding();
        let version = document.version();
        let actions: Result<Vec<_>> = params
            .context
            .diagnostics
            .into_iter()
            .map(|diagnostic| {
                let Some(data) = diagnostic.data else {
                    return Ok(None);
                };
                let diagnostic_fix: crate::lint::DiagnosticFix = serde_json::from_value(data)
                    .map_err(|err| anyhow::anyhow!("failed to deserialize diagnostic data: {err}"))
                    .with_failure_code(lsp_server::ErrorCode::ParseError)?;
                let edits = diagnostic_fix
                    .fix
                    .edits()
                    .iter()
                    .map(|edit| types::TextEdit {
                        range: edit.range().to_range(
                            document.contents(),
                            document.index(),
                            encoding,
                        ),
                        new_text: edit.content().unwrap_or_default().to_string(),
                    });

                let changes = vec![types::TextDocumentEdit {
                    text_document: types::OptionalVersionedTextDocumentIdentifier::new(
                        url.clone(),
                        version,
                    ),
                    edits: edits.map(types::OneOf::Left).collect(),
                }];

                let title = diagnostic_fix
                    .kind
                    .suggestion
                    .unwrap_or(diagnostic_fix.kind.name);
                Ok(Some(types::CodeAction {
                    title,
                    kind: Some(types::CodeActionKind::QUICKFIX),
                    edit: Some(types::WorkspaceEdit {
                        document_changes: Some(types::DocumentChanges::Edits(changes)),
                        ..Default::default()
                    }),
                    ..Default::default()
                }))
            })
            .collect();

        Ok(Some(
            actions?
                .into_iter()
                .flatten()
                .map(types::CodeActionOrCommand::CodeAction)
                .collect(),
        ))
    }
}
