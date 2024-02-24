use crate::server::api::LSPResult;
use crate::server::client::Notifier;
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChange;

impl super::Notification for DidChange {
    type NotificationType = notif::DidChangeTextDocument;
}

impl super::SyncNotification for DidChange {
    #[tracing::instrument(skip_all, fields(file=%params.text_document.uri))]
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        params: types::DidChangeTextDocumentParams,
    ) -> Result<()> {
        super::define_document_url!(params: &types::DidChangeTextDocumentParams);

        if params.content_changes.is_empty() {
            return Ok(());
        }

        let new_version = params.text_document.version;

        let encoding = session.encoding();

        let document = session
            .document_controller(document_url(&params))
            .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;

        document.apply_changes(params.content_changes, new_version, encoding);

        Ok(())
    }
}
