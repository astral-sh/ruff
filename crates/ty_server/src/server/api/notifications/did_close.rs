use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_server::ErrorCode;
use lsp_types::DidCloseTextDocumentParams;
use lsp_types::notification::DidCloseTextDocument;
use ty_project::watch::ChangeEvent;

pub(crate) struct DidCloseTextDocumentHandler;

impl NotificationHandler for DidCloseTextDocumentHandler {
    type NotificationType = DidCloseTextDocument;
}

impl SyncNotificationHandler for DidCloseTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: DidCloseTextDocumentParams,
    ) -> Result<()> {
        let Ok(key) = session.key_from_url(params.text_document.uri.clone()) else {
            tracing::debug!(
                "Failed to create document key from URI: {}",
                params.text_document.uri
            );
            return Ok(());
        };
        session
            .close_document(&key)
            .with_failure_code(ErrorCode::InternalError)?;

        if let AnySystemPath::SystemVirtual(virtual_path) = key.path() {
            let db = session.default_project_db_mut();
            db.apply_changes(
                vec![ChangeEvent::DeletedVirtual(virtual_path.clone())],
                None,
            );
        }

        clear_diagnostics(&key, client)
    }
}
