use lsp_server::ErrorCode;
use lsp_types::DidOpenNotebookDocumentParams;
use lsp_types::notification::DidOpenNotebookDocument;

use crate::TextDocument;
use crate::document::NotebookDocument;
use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::notifications::did_open::opened_document;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

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
            uri: notebook_uri,
            ..
        } = params.notebook_document;

        let notebook =
            NotebookDocument::new(notebook_uri, version, cells, metadata.unwrap_or_default())
                .with_failure_code(ErrorCode::InternalError)?;

        let document = session.open_notebook_document(notebook);
        let path = document.notebook_or_file_path();

        for cell in params.cell_text_documents {
            let cell_document = TextDocument::new(cell.uri, cell.text, cell.version)
                .with_language_id(&cell.language_id)
                .with_notebook(path.clone());
            session.open_text_document(cell_document);
        }

        opened_document(&document, session, client);

        Ok(())
    }
}
