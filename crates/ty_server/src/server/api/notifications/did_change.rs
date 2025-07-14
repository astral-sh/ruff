use lsp_server::ErrorCode;
use lsp_types::notification::DidChangeTextDocument;
use lsp_types::{DidChangeTextDocumentParams, VersionedTextDocumentIdentifier};

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use ty_project::watch::ChangeEvent;

pub(crate) struct DidChangeTextDocumentHandler;

impl NotificationHandler for DidChangeTextDocumentHandler {
    type NotificationType = DidChangeTextDocument;
}

impl SyncNotificationHandler for DidChangeTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: DidChangeTextDocumentParams,
    ) -> Result<()> {
        let DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes,
        } = params;

        let key = match session.key_from_url(uri) {
            Ok(key) => key,
            Err(uri) => {
                tracing::debug!("Failed to create document key from URI: {}", uri);
                return Ok(());
            }
        };

        session
            .update_text_document(&key, content_changes, version)
            .with_failure_code(ErrorCode::InternalError)?;

        let changes = match key.path() {
            AnySystemPath::System(system_path) => {
                vec![ChangeEvent::file_content_changed(system_path.clone())]
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                vec![ChangeEvent::ChangedVirtual(virtual_path.clone())]
            }
        };

        session.apply_changes(key.path(), changes);

        publish_diagnostics(session, &key, client);

        Ok(())
    }
}
