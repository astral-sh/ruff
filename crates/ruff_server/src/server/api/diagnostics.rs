use lsp_types::Url;

use crate::{
    Session,
    lint::DiagnosticsMap,
    session::{Client, DocumentQuery, DocumentSnapshot},
};

use super::LSPResult;

pub(super) fn generate_diagnostics(snapshot: &DocumentSnapshot) -> DiagnosticsMap {
    if snapshot.client_settings().lint() {
        let document = snapshot.query();
        crate::lint::check(
            document,
            snapshot.encoding(),
            snapshot.client_settings().show_syntax_errors(),
        )
    } else {
        DiagnosticsMap::default()
    }
}

pub(super) fn publish_diagnostics_for_document(
    session: &Session,
    url: &Url,
    client: &Client,
) -> crate::server::Result<()> {
    // Publish diagnostics if the client doesn't support pull diagnostics
    if session.resolved_client_capabilities().pull_diagnostics {
        return Ok(());
    }

    let snapshot = session
        .take_snapshot(url.clone())
        .ok_or_else(|| anyhow::anyhow!("Unable to take snapshot for document with URL {url}"))
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    for (uri, diagnostics) in generate_diagnostics(&snapshot) {
        client
            .send_notification::<lsp_types::notification::PublishDiagnostics>(
                lsp_types::PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version: Some(snapshot.query().version()),
                },
            )
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;
    }

    Ok(())
}

pub(super) fn clear_diagnostics_for_document(
    session: &Session,
    query: &DocumentQuery,
    client: &Client,
) -> crate::server::Result<()> {
    if session.resolved_client_capabilities().pull_diagnostics {
        return Ok(());
    }

    client
        .send_notification::<lsp_types::notification::PublishDiagnostics>(
            lsp_types::PublishDiagnosticsParams {
                uri: query.make_key().into_url(),
                diagnostics: vec![],
                version: Some(query.version()),
            },
        )
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    Ok(())
}
