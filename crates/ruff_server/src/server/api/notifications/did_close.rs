use crate::server::api::diagnostics::clear_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidClose;

impl super::NotificationHandler for DidClose {
    type NotificationType = notif::DidCloseTextDocument;
}

impl super::SyncNotificationHandler for DidClose {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidCloseTextDocumentParams {
            text_document: types::TextDocumentIdentifier { uri },
        }: types::DidCloseTextDocumentParams,
    ) -> Result<()> {
        // Publish an empty diagnostic report for the document. This will de-register any existing diagnostics.
        let snapshot = session
            .take_snapshot(uri.clone())
            .ok_or_else(|| anyhow::anyhow!("Unable to take snapshot for document with URL {uri}"))
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        clear_diagnostics_for_document(snapshot.query(), &notifier)?;

        let key = snapshot.query().make_key();

        session
            .close_document(&key)
            .with_failure_code(lsp_server::ErrorCode::InternalError)
    }
}
