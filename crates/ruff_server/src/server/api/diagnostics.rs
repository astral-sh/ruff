use crate::{
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
            snapshot
                .resolved_client_capabilities()
                .diagnostic_related_information,
        )
    } else {
        DiagnosticsMap::default()
    }
}

pub(super) fn publish_diagnostics_for_document(
    snapshot: &DocumentSnapshot,
    client: &Client,
) -> crate::server::Result<()> {
    #[expect(
        clippy::iter_over_hash_type,
        reason = "diagnostic notifications for distinct document URIs are independent"
    )]
    for (uri, diagnostics) in generate_diagnostics(snapshot) {
        client
            .send_notification::<lsp_types::PublishDiagnosticsNotification>(
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
    query: &DocumentQuery,
    client: &Client,
) -> crate::server::Result<()> {
    client
        .send_notification::<lsp_types::PublishDiagnosticsNotification>(
            lsp_types::PublishDiagnosticsParams {
                uri: query.make_key().into_uri(),
                diagnostics: vec![],
                version: Some(query.version()),
            },
        )
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;

    Ok(())
}
