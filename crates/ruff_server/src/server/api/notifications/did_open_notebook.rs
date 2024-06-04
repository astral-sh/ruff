use crate::edit::NotebookDocument;
use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_server::ErrorCode;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidOpenNotebook;

impl super::NotificationHandler for DidOpenNotebook {
    type NotificationType = notif::DidOpenNotebookDocument;
}

impl super::SyncNotificationHandler for DidOpenNotebook {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidOpenNotebookDocumentParams {
            notebook_document:
                types::NotebookDocument {
                    uri,
                    version,
                    cells,
                    metadata,
                    ..
                },
            cell_text_documents,
        }: types::DidOpenNotebookDocumentParams,
    ) -> Result<()> {
        let notebook = NotebookDocument::new(
            version,
            cells,
            metadata.unwrap_or_default(),
            cell_text_documents,
        )
        .with_failure_code(ErrorCode::InternalError)?;

        session.open_notebook_document(uri.clone(), notebook);

        // publish diagnostics
        let snapshot = session
            .take_snapshot(uri)
            .expect("snapshot should be available");
        publish_diagnostics_for_document(&snapshot, &notifier)?;

        Ok(())
    }
}
