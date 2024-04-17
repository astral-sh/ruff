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
        _notifier: Notifier,
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
        Ok(())
    }
}
