use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::session::{Client, Session};
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
        client: &Client,
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

        publish_diagnostics_for_document(session, &key.into_url(), client)?;

        Ok(())
    }
}
