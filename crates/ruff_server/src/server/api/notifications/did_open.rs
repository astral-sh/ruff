use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidOpen;

impl super::NotificationHandler for DidOpen {
    type NotificationType = notif::DidOpenTextDocument;
}

impl super::SyncNotificationHandler for DidOpen {
    #[tracing::instrument(skip_all, fields(file=%url))]
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidOpenTextDocumentParams {
            text_document:
                types::TextDocumentItem {
                    uri: ref url,
                    text,
                    version,
                    ..
                },
        }: types::DidOpenTextDocumentParams,
    ) -> Result<()> {
        session.open_document(url, text, version);

        // Publish diagnostics if the client doesnt support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session
                .take_snapshot(url)
                .ok_or_else(|| {
                    anyhow::anyhow!("Unable to take snapshot for document with URL {url}")
                })
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            publish_diagnostics_for_document(&snapshot, &notifier)?;
        }

        Ok(())
    }
}
