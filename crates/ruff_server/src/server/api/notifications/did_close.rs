use crate::server::api::LSPResult;
use crate::server::client::Notifier;
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidClose;

impl super::Notification for DidClose {
    type NotificationType = notif::DidCloseTextDocument;
}

impl super::SyncNotification for DidClose {
    #[tracing::instrument(skip_all, fields(file=%uri))]
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        types::DidCloseTextDocumentParams {
            text_document: types::TextDocumentIdentifier { uri },
        }: types::DidCloseTextDocumentParams,
    ) -> Result<()> {
        session
            .close_document(&uri)
            .with_failure_code(lsp_server::ErrorCode::InternalError)
    }
}
