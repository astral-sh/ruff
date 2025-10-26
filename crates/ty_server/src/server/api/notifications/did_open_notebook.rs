use lsp_server::ErrorCode;
use lsp_types::DidOpenNotebookDocumentParams;
use lsp_types::notification::DidOpenNotebookDocument;

use ruff_db::Db;
use ty_project::watch::ChangeEvent;

use crate::TextDocument;
use crate::document::NotebookDocument;
use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics;
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
        client: &Client,
        params: DidOpenNotebookDocumentParams,
    ) -> Result<()> {
        let lsp_types::NotebookDocument {
            version,
            cells,
            metadata,
            ..
        } = params.notebook_document;

        let notebook = NotebookDocument::new(
            params.notebook_document.uri,
            version,
            cells,
            metadata.unwrap_or_default(),
        )
        .with_failure_code(ErrorCode::InternalError)?;

        let document = session.open_notebook_document(notebook);
        let path = document.to_file_path();

        for cell in params.cell_text_documents {
            let cell_document = TextDocument::new(cell.uri, cell.text, cell.version)
                .with_language_id(&cell.language_id)
                .with_notebook(path.clone().into_owned());
            session.open_text_document(cell_document);
        }

        match &*path {
            AnySystemPath::System(system_path) => {
                session.apply_changes(&path, vec![ChangeEvent::Opened(system_path.clone())]);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.project_db_mut(&path);
                db.files().virtual_file(db, virtual_path);
            }
        }

        publish_diagnostics(session, document.url(), client);

        Ok(())
    }
}
