use lsp_types::DidCloseNotebookDocumentParams;
use lsp_types::notification::DidCloseNotebookDocument;

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use ty_project::watch::ChangeEvent;

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
        let Ok(key) = session.key_from_url(params.notebook_document.uri.clone()) else {
            tracing::debug!(
                "Failed to create document key from URI: {}",
                params.notebook_document.uri
            );
            return Ok(());
        };
        session
            .close_document(&key)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        if let AnySystemPath::SystemVirtual(virtual_path) = key.path() {
            let db = session.default_project_db_mut();
            db.apply_changes(
                vec![ChangeEvent::DeletedVirtual(virtual_path.clone())],
                None,
            );
        }

        Ok(())
    }
}
