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

        let document = session
            .document_handle(&uri)
            .with_failure_code(ErrorCode::InternalError)?;

        document
            .update_text_document(session, content_changes, version)
            .with_failure_code(ErrorCode::InternalError)?;

        let path = document.to_file_path();
        let changes = match &*path {
            AnySystemPath::System(system_path) => {
                vec![ChangeEvent::file_content_changed(system_path.clone())]
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                vec![ChangeEvent::ChangedVirtual(virtual_path.clone())]
            }
        };

        session.apply_changes(&path, changes);

        publish_diagnostics(session, document.url(), client);

        Ok(())
    }
}
