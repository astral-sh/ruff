use lsp_server::ErrorCode;
use lsp_types::notification::DidChangeTextDocument;
use lsp_types::{DidChangeTextDocumentParams, VersionedTextDocumentIdentifier};

use ty_project::watch::ChangeEvent;

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::client::{Notifier, Requester};
use crate::session::Session;
use crate::system::{AnySystemPath, url_to_any_system_path};

pub(crate) struct DidChangeTextDocumentHandler;

impl NotificationHandler for DidChangeTextDocumentHandler {
    type NotificationType = DidChangeTextDocument;
}

impl SyncNotificationHandler for DidChangeTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
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

        match path.clone() {
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

        // Publish diagnostics if the client doesn't support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let db = path
                .as_system()
                .and_then(|path| session.project_db_for_path(path.as_std_path()))
                .unwrap_or_else(|| session.default_project_db());
            let snapshot = session
                .take_snapshot(uri.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("Unable to take snapshot for document with URL {uri}")
                })
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            publish_diagnostics_for_document(db, &snapshot, &notifier)?;
        }

        Ok(())
    }
}
