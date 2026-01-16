use lsp_types::DidSaveTextDocumentParams;
use lsp_types::notification::DidSaveTextDocument;

use crate::server::Result;
use crate::server::api::diagnostics::publish_diagnostics_if_needed;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidSaveTextDocumentHandler;

impl NotificationHandler for DidSaveTextDocumentHandler {
    type NotificationType = DidSaveTextDocument;
}

impl SyncNotificationHandler for DidSaveTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: DidSaveTextDocumentParams,
    ) -> Result<()> {
        let DidSaveTextDocumentParams {
            text_document: _,
            text: _,
        } = params;

        for document in session.text_document_handles() {
            publish_diagnostics_if_needed(&document, session, client);
        }

        Ok(())
    }
}
