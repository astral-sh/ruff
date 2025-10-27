use lsp_types::notification::DidCloseNotebookDocument;
use lsp_types::{DidCloseNotebookDocumentParams, NotebookDocumentIdentifier};

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
        let DidCloseNotebookDocumentParams {
            notebook_document: NotebookDocumentIdentifier { uri },
            ..
        } = params;

        let document = session
            .document_handle(&uri)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        let path = document.to_file_path().into_owned();

        document
            .close(session)
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;

        if let AnySystemPath::SystemVirtual(virtual_path) = &path {
            session.apply_changes(
                &path,
                vec![ChangeEvent::DeletedVirtual(virtual_path.clone())],
            );
        }

        Ok(())
    }
}
