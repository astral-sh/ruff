use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};

use ruff_db::Db;
use ty_project::watch::ChangeEvent;

use crate::TextDocument;
use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::client::{Notifier, Requester};
use crate::session::Session;
use crate::system::{AnySystemPath, url_to_any_system_path};

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        DidOpenTextDocumentParams {
            text_document:
                TextDocumentItem {
                    uri,
                    text,
                    version,
                    language_id,
                },
        }: DidOpenTextDocumentParams,
    ) -> Result<()> {
        let Ok(path) = url_to_any_system_path(&uri) else {
            return Ok(());
        };

        let document = TextDocument::new(text, version).with_language_id(&language_id);
        session.open_text_document(uri.clone(), document);

        match &path {
            AnySystemPath::System(path) => {
                let db = match session.project_db_for_path_mut(path.as_std_path()) {
                    Some(db) => db,
                    None => session.default_project_db_mut(),
                };
                db.apply_changes(vec![ChangeEvent::Opened(path.clone())], None);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.default_project_db_mut();
                db.files().virtual_file(db, virtual_path);
            }
        }

        // Publish diagnostics if the client doesn't support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session
                .take_snapshot(uri.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("Unable to take snapshot for document with URL {uri}")
                })
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            publish_diagnostics_for_document(
                session.project_db_or_default(&path),
                &snapshot,
                &notifier,
            )?;
        }

        Ok(())
    }
}
