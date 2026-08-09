use lsp_types::{DidSaveTextDocumentNotification, DidSaveTextDocumentParams};
use ty_project::ScriptEnvironmentAvailability;

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
        params: DidSaveTextDocumentParams,
    ) -> Result<()> {
        if let Ok(document) = session.document_handle(&params.text_document.uri) {
            // Keep diagnostics visible if unsaved edits first turned this file into a script.
            session.synchronize_script(
                client,
                document.notebook_or_file_path(),
                ScriptEnvironmentAvailability::Available,
            );
        }

        for document in session.file_document_handles() {
            publish_diagnostics_if_needed(&document, session, client);
        }

        Ok(())
    }
}
