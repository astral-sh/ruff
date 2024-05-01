use crate::{server::client::Notifier, session::DocumentSnapshot};

use super::LSPResult;

pub(super) fn generate_diagnostics(snapshot: &DocumentSnapshot) -> Vec<lsp_types::Diagnostic> {
    if snapshot.client_settings().lint() {
        crate::lint::check(
            snapshot.document(),
            snapshot.url(),
            snapshot.settings().linter(),
            snapshot.encoding(),
        )
    } else {
        vec![]
    }
}

pub(super) fn publish_diagnostics_for_document(
    snapshot: &DocumentSnapshot,
    notifier: &Notifier,
) -> crate::server::Result<()> {
    let diagnostics = generate_diagnostics(snapshot);

    notifier
        .notify::<lsp_types::notification::PublishDiagnostics>(
            lsp_types::PublishDiagnosticsParams {
                uri: snapshot.url().clone(),
                diagnostics,
                version: Some(snapshot.document().version()),
            },
        )
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    Ok(())
}

pub(super) fn clear_diagnostics_for_document(
    snapshot: &DocumentSnapshot,
    notifier: &Notifier,
) -> crate::server::Result<()> {
    notifier
        .notify::<lsp_types::notification::PublishDiagnostics>(
            lsp_types::PublishDiagnosticsParams {
                uri: snapshot.url().clone(),
                diagnostics: vec![],
                version: Some(snapshot.document().version()),
            },
        )
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    Ok(())
}
