use crate::edit::text_range_to_range;
use crate::server::{client::Notifier, Result};
use crate::session::SessionSnapshot;
use lsp_types::{self as types, request as req};
use ruff_text_size::Ranged;

pub(crate) struct CodeAction;

impl super::Request for CodeAction {
    type RequestType = req::CodeActionRequest;
}

impl super::BackgroundRequest for CodeAction {
    super::define_document_url!(params: &types::CodeActionParams);
    fn run_with_snapshot(
        snapshot: SessionSnapshot,
        _notifier: Notifier,
        params: types::CodeActionParams,
    ) -> Result<Option<types::CodeActionResponse>> {
        let document = snapshot.document();
        let url = snapshot.url();
        let encoding = snapshot.encoding();
        let version = document.version();
        let actions = params
            .context
            .diagnostics
            .into_iter()
            .filter_map(|diagnostic| {
                let diagnostic_fix: crate::lint::DiagnosticFix =
                    serde_json::from_value(diagnostic.data?).ok()?;
                let edits = diagnostic_fix
                    .fix
                    .edits()
                    .iter()
                    .map(|edit| types::TextEdit {
                        range: text_range_to_range(edit.range(), document, encoding),
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
                Some(types::CodeAction {
                    title,
                    kind: Some(types::CodeActionKind::QUICKFIX),
                    edit: Some(types::WorkspaceEdit {
                        document_changes: Some(types::DocumentChanges::Edits(changes)),
                        ..Default::default()
                    }),
                    ..Default::default()
                })
            });

        Ok(Some(
            actions
                .map(types::CodeActionOrCommand::CodeAction)
                .collect(),
        ))
    }
}
