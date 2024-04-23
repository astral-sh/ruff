use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChange;

impl super::NotificationHandler for DidChange {
    type NotificationType = notif::DidChangeTextDocument;
}

impl super::SyncNotificationHandler for DidChange {
    #[tracing::instrument(skip_all, fields(file=%uri))]
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
        let encoding = session.encoding();
        let document = session
            .document_controller(&uri)
            .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;

        if content_changes.is_empty() {
            document.make_mut().update_version(new_version);
            return Ok(());
        }

        document
            .make_mut()
            .apply_changes(content_changes, new_version, encoding);

        // Publish diagnostics if the client doesnt support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session.take_snapshot(&uri).unwrap();
            publish_diagnostics_for_document(&snapshot, &notifier)?;
        }

        Ok(())
    }
}
