use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeNotebook;

impl super::NotificationHandler for DidChangeNotebook {
    type NotificationType = notif::DidChangeNotebookDocument;
}

impl super::SyncNotificationHandler for DidChangeNotebook {
    #[tracing::instrument(skip_all, fields(file=%uri))]
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidChangeNotebookDocumentParams {
            notebook_document:
                types::VersionedNotebookDocumentIdentifier {
                    uri,
                    version: new_version,
                },
            change,
        }: types::DidChangeNotebookDocumentParams,
    ) -> Result<()> {
        tracing::info!("Notebook Changed: {}", uri);
        show_err_msg!("Notebook Changed");
        Ok(())
    }
}
