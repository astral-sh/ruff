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
    #[tracing::instrument(skip_all, fields(file=%params.text_document.uri))]
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        params: types::DidCloseTextDocumentParams,
    ) -> Result<()> {
        super::define_document_url!(params: &types::DidCloseTextDocumentParams);
        session
            .close_document(document_url(&params))
            .with_failure_code(lsp_server::ErrorCode::InternalError)
    }
}
