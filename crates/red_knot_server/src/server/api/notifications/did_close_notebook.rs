use lsp_types::notification::DidCloseNotebookDocument;
use lsp_types::DidCloseNotebookDocumentParams;
use ruff_db::files::system_path_to_file;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::url_to_system_path;

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
        let Ok(path) = url_to_system_path(&params.notebook_document.uri) else {
            return Ok(());
        };

        let key = session.key_from_url(params.notebook_document.uri);
        session
            .close_document(&key)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        if let Some(db) = session.workspace_db_for_path_mut(path.as_std_path()) {
            let file = system_path_to_file(&**db, path).unwrap();
            db.workspace().close_file(db.get_mut(), file);
        }

        Ok(())
    }
}
