use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidOpenNotebook;

impl super::NotificationHandler for DidOpenNotebook {
    type NotificationType = notif::DidOpenNotebookDocument;
}

impl super::SyncNotificationHandler for DidOpenNotebook {
    #[tracing::instrument(skip_all, fields(file=%url))]
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidOpenNotebookDocumentParams {
            notebook_document:
                types::NotebookDocument {
                    uri: url,
                    notebook_type,
                    version,
                    metadata,
                    cells,
                },
            cell_text_documents: text_items,
        }: types::DidOpenNotebookDocumentParams,
    ) -> Result<()> {
        tracing::info!("DidOpenNotebook: {}", url);
        show_err_msg!("DidOpenNotebook");

        Ok(())
    }
}
