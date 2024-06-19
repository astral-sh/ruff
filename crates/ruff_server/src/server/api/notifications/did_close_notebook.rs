use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidCloseNotebook;

impl super::NotificationHandler for DidCloseNotebook {
    type NotificationType = notif::DidCloseNotebookDocument;
}

impl super::SyncNotificationHandler for DidCloseNotebook {
    fn run(
        _session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        _params: types::DidCloseNotebookDocumentParams,
    ) -> Result<()> {
        // `textDocument/didClose` is called after didCloseNotebook,
        // and the document is removed from the index at that point.
        // For this specific notification, we don't need to do anything.
        Ok(())
    }
}
