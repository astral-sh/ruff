use crate::server::api::LSPResult;
use crate::server::client::Notifier;
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
        _notifier: Notifier,
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

        Ok(())
    }
}
