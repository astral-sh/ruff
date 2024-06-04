use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_server::ErrorCode;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeNotebook;

impl super::NotificationHandler for DidChangeNotebook {
    type NotificationType = notif::DidChangeNotebookDocument;
}

impl super::SyncNotificationHandler for DidChangeNotebook {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidChangeNotebookDocumentParams {
            notebook_document: types::VersionedNotebookDocumentIdentifier { uri, version },
            change: types::NotebookDocumentChangeEvent { cells, metadata },
        }: types::DidChangeNotebookDocumentParams,
    ) -> Result<()> {
        let key = session.key_from_url(uri);
        session
            .update_notebook_document(&key, cells, metadata, version)
            .with_failure_code(ErrorCode::InternalError)?;

        // publish new diagnostics
        let snapshot = session
            .take_snapshot(key.into_url())
            .expect("snapshot should be available");
        publish_diagnostics_for_document(&snapshot, &notifier)?;

        Ok(())
    }
}
