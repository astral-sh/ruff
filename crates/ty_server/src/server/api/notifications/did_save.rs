use lsp_types::{DidSaveTextDocumentNotification, DidSaveTextDocumentParams};

use crate::server::Result;
use crate::server::api::diagnostics::publish_diagnostics_if_needed;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidSaveTextDocumentHandler;

impl NotificationHandler for DidSaveTextDocumentHandler {
    type NotificationType = DidSaveTextDocumentNotification;
}

impl SyncNotificationHandler for DidSaveTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        _params: DidSaveTextDocumentParams,
    ) -> Result<()> {
        for document in session.file_document_handles() {
            publish_diagnostics_if_needed(&document, session, client);
        }

        Ok(())
    }
}
