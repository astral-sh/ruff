use crate::edit::NotebookDocument;
use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use anyhow::anyhow;
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
                    notebook_type,
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

        let notebook_path = uri
            .to_file_path()
            .map_err(|()| anyhow!("expected notebook URI {uri} to be a valid file path"))
            .with_failure_code(ErrorCode::InvalidParams)?;

        session.open_notebook_document(notebook_path, notebook);

        // publish diagnostics
        let snapshot = session
            .take_snapshot(&uri)
            .expect("snapshot should be available");
        publish_diagnostics_for_document(&snapshot, &notifier)?;

        Ok(())
    }
}
