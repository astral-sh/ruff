use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::{DidCloseTextDocumentParams, TextDocumentIdentifier};
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
        let DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        } = params;

        let key = match session.key_from_url(uri) {
            Ok(key) => key,
            Err(uri) => {
                tracing::debug!("Failed to create document key from URI: {}", uri);
                return Ok(());
            }
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

        if !session.global_settings().diagnostic_mode().is_workspace() {
            // The server needs to clear the diagnostics regardless of whether the client supports
            // pull diagnostics or not. This is because the client only has the capability to fetch
            // the diagnostics but does not automatically clear them when a document is closed.
            clear_diagnostics(&key, client);
        }

        Ok(())
    }
}
