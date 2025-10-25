use lsp_server::ErrorCode;
use lsp_types::DidOpenNotebookDocumentParams;
use lsp_types::notification::DidOpenNotebookDocument;

use ruff_db::Db;
use ty_project::watch::ChangeEvent;

use crate::document::NotebookDocument;
use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;

pub(crate) struct DidOpenNotebookHandler;

impl NotificationHandler for DidOpenNotebookHandler {
    type NotificationType = DidOpenNotebookDocument;
}

impl SyncNotificationHandler for DidOpenNotebookHandler {
    fn run(
        session: &mut Session,
        _client: &Client,
        params: DidOpenNotebookDocumentParams,
    ) -> Result<()> {
        let path = AnySystemPath::from_url(&params.notebook_document.uri);

        let lsp_types::NotebookDocument {
            version,
            cells,
            metadata,
            ..
        } = params.notebook_document;

        let notebook = NotebookDocument::new(
            version,
            cells,
            metadata.unwrap_or_default(),
            params.cell_text_documents,
        )
        .with_failure_code(ErrorCode::InternalError)?;
        session.open_notebook_document(&path, notebook);

        match &path {
            AnySystemPath::System(system_path) => {
                session.apply_changes(&path, vec![ChangeEvent::Opened(system_path.clone())]);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.project_db_mut(&path);
                db.files().virtual_file(db, virtual_path);
            }
        }

        // TODO(dhruvmanila): Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
