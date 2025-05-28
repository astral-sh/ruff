use lsp_server::ErrorCode;
use lsp_types::notification::DidChangeTextDocument;
use lsp_types::{DidChangeTextDocumentParams, VersionedTextDocumentIdentifier};

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::{AnySystemPath, url_to_any_system_path};
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

        let Ok(path) = url_to_any_system_path(&uri) else {
            return Ok(());
        };

        let key = session.key_from_url(uri.clone());

        session
            .update_text_document(&key, content_changes, version)
            .with_failure_code(ErrorCode::InternalError)?;

        match path {
            AnySystemPath::System(path) => {
                let db = match session.project_db_for_path_mut(path.as_std_path()) {
                    Some(db) => db,
                    None => session.default_project_db_mut(),
                };
                db.apply_changes(vec![ChangeEvent::file_content_changed(path)], None);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.default_project_db_mut();
                db.apply_changes(vec![ChangeEvent::ChangedVirtual(virtual_path)], None);
            }
        }

        publish_diagnostics(session, uri, client)
    }
}
