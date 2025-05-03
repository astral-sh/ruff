use lsp_types::notification::DidCloseNotebookDocument;
use lsp_types::DidCloseNotebookDocumentParams;

use ty_project::watch::ChangeEvent;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::{url_to_any_system_path, AnySystemPath};

pub(crate) struct DidCloseNotebookHandler;

impl NotificationHandler for DidCloseNotebookHandler {
    type NotificationType = DidCloseNotebookDocument;
}

impl SyncNotificationHandler for DidCloseNotebookHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DidCloseNotebookDocumentParams,
    ) -> Result<()> {
        let Ok(path) = url_to_any_system_path(&params.notebook_document.uri) else {
            return Ok(());
        };

        let key = session.key_from_url(params.notebook_document.uri);
        session
            .close_document(&key)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        if let AnySystemPath::SystemVirtual(virtual_path) = path {
            let db = session.default_project_db_mut();
            db.apply_changes(vec![ChangeEvent::DeletedVirtual(virtual_path)], None);
        }

        Ok(())
    }
}
