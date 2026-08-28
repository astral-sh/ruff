use lsp_types::{DidSaveTextDocumentNotification, DidSaveTextDocumentParams};
use ty_project::ScriptEnvironmentAvailability;

use crate::server::Result;
use crate::server::api::diagnostics::publish_all_document_diagnostics;
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
        params: DidSaveTextDocumentParams,
    ) -> Result<()> {
        if let Ok(document) = session.document_handle(&params.text_document.uri) {
            // Keep diagnostics visible if unsaved edits first turned this file into a script.
            document.synchronize_script(session, client, ScriptEnvironmentAvailability::Available);
        }

        publish_all_document_diagnostics(session, client);

        Ok(())
    }
}
