use lsp_server::ErrorCode;
use lsp_types as types;
use lsp_types::notification as notif;

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::notifications::did_change::file_changed;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidChangeNotebookHandler;

impl NotificationHandler for DidChangeNotebookHandler {
    type NotificationType = notif::DidChangeNotebookDocument;
}

impl SyncNotificationHandler for DidChangeNotebookHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        types::DidChangeNotebookDocumentParams {
            notebook_document: types::VersionedNotebookDocumentIdentifier { uri, version },
            change: types::NotebookDocumentChangeEvent { cells, metadata },
        }: types::DidChangeNotebookDocumentParams,
    ) -> Result<()> {
        let document = session
            .document_handle(&uri)
            .with_failure_code(ErrorCode::InternalError)?;

        document
            .update_notebook_document(session, cells, metadata, version)
            .with_failure_code(ErrorCode::InternalError)?;

        file_changed(&document, session, client);

        Ok(())
    }
}
