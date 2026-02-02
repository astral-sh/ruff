use crate::server::Result;
use crate::server::api::LSPResult;
use crate::session::{Client, Session};
use lsp_types::notification as notif;
use lsp_types::{self as types, NotebookDocumentIdentifier};

pub(crate) struct DidCloseNotebook;

impl super::NotificationHandler for DidCloseNotebook {
    type NotificationType = notif::DidCloseNotebookDocument;
}

impl super::SyncNotificationHandler for DidCloseNotebook {
    fn run(
        session: &mut Session,
        _client: &Client,
        types::DidCloseNotebookDocumentParams {
            notebook_document: NotebookDocumentIdentifier { uri },
            ..
        }: types::DidCloseNotebookDocumentParams,
    ) -> Result<()> {
        let key = session.key_from_url(uri);
        session
            .close_document(&key)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        Ok(())
    }
}
