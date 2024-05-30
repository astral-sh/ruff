use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_server::ErrorCode;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChange;

impl super::NotificationHandler for DidChange {
    type NotificationType = notif::DidChangeTextDocument;
}

impl super::SyncNotificationHandler for DidChange {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidChangeTextDocumentParams {
            text_document:
                types::VersionedTextDocumentIdentifier {
                    uri,
                    version: new_version,
                },
            content_changes,
        }: types::DidChangeTextDocumentParams,
    ) -> Result<()> {
        let key = session.key_from_url(uri);

        session
            .update_text_document(&key, content_changes, new_version)
            .with_failure_code(ErrorCode::InternalError)?;

        // Publish diagnostics if the client doesnt support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session.take_snapshot(key.into_url()).unwrap();
            publish_diagnostics_for_document(&snapshot, &notifier)?;
        }

        Ok(())
    }
}
