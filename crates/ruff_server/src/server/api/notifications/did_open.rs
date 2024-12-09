use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::TextDocument;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidOpen;

impl super::NotificationHandler for DidOpen {
    type NotificationType = notif::DidOpenTextDocument;
}

impl super::SyncNotificationHandler for DidOpen {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidOpenTextDocumentParams {
            text_document:
                types::TextDocumentItem {
                    uri,
                    text,
                    version,
                    language_id,
                },
        }: types::DidOpenTextDocumentParams,
    ) -> Result<()> {
        let document = TextDocument::new(text, version).with_language_id(&language_id);

        session.open_text_document(uri.clone(), document);

        // Publish diagnostics if the client doesn't support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session
                .take_snapshot(uri.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("Unable to take snapshot for document with URL {uri}")
                })
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            publish_diagnostics_for_document(&snapshot, &notifier)?;
        }

        Ok(())
    }
}
