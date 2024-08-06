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

        // TODO(dhruvmanila): Close the file in the `RootDatabase`

        Ok(())
    }
}
