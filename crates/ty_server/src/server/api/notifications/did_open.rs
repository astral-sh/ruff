use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};

use crate::TextDocument;
use crate::server::Result;
use crate::server::api::diagnostics::publish_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use ruff_db::Db;
use ty_project::watch::ChangeEvent;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        let DidOpenTextDocumentParams {
            text_document:
                TextDocumentItem {
                    uri,
                    text,
                    version,
                    language_id,
                },
        } = params;

        let key = match session.key_from_url(uri) {
            Ok(key) => key,
            Err(uri) => {
                tracing::debug!("Failed to create document key from URI: {}", uri);
                return Ok(());
            }
        };

        let document = TextDocument::new(text, version).with_language_id(&language_id);
        session.open_text_document(key.path(), document);

        match key.path() {
            AnySystemPath::System(system_path) => {
                session.apply_changes(key.path(), vec![ChangeEvent::Opened(system_path.clone())]);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.default_project_db_mut();
                db.files().virtual_file(db, virtual_path);
            }
        }

        publish_diagnostics(session, &key, client);

        Ok(())
    }
}
