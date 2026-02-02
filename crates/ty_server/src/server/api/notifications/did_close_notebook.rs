use lsp_types::notification::DidCloseNotebookDocument;
use lsp_types::{DidCloseNotebookDocumentParams, NotebookDocumentIdentifier};

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidCloseNotebookHandler;

impl NotificationHandler for DidCloseNotebookHandler {
    type NotificationType = DidCloseNotebookDocument;
}

impl SyncNotificationHandler for DidCloseNotebookHandler {
    fn run(
        session: &mut Session,
        _client: &Client,
        params: DidCloseNotebookDocumentParams,
    ) -> Result<()> {
        let DidCloseNotebookDocumentParams {
            notebook_document: NotebookDocumentIdentifier { uri },
            ..
        } = params;

        let document = session
            .document_handle(&uri)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        // We don't need to call publish any diagnostics because we clear
        // the diagnostics when closing the corresponding cell documents.
        let _ = document
            .close(session)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        Ok(())
    }
}
