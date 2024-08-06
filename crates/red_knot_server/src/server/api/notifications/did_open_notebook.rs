use lsp_server::ErrorCode;
use lsp_types::notification::DidOpenNotebookDocument;
use lsp_types::DidOpenNotebookDocumentParams;

use ruff_db::files::system_path_to_file;

use crate::edit::NotebookDocument;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::url_to_system_path;

pub(crate) struct DidOpenNotebookHandler;

impl NotificationHandler for DidOpenNotebookHandler {
    type NotificationType = DidOpenNotebookDocument;
}

impl SyncNotificationHandler for DidOpenNotebookHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DidOpenNotebookDocumentParams,
    ) -> Result<()> {
        let Ok(path) = url_to_system_path(&params.notebook_document.uri) else {
            return Ok(());
        };

        let notebook = NotebookDocument::new(
            params.notebook_document.version,
            params.notebook_document.cells,
            params.notebook_document.metadata.unwrap_or_default(),
            params.cell_text_documents,
        )
        .with_failure_code(ErrorCode::InternalError)?;
        session.open_notebook_document(params.notebook_document.uri.clone(), notebook);

        if let Some(db) = session.workspace_db_for_path_mut(path.as_std_path()) {
            // TODO(dhruvmanila): Store the `file` in `DocumentController`
            let file = system_path_to_file(&**db, &path).unwrap();
            file.sync(db.get_mut());
        }

        // TODO(dhruvmanila): Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
