use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};
use ruff_db::Db as _;
use ruff_db::files::system_path_to_file;
use ty_project::Db as _;
use ty_project::watch::ChangeEvent;

use crate::TextDocument;
use crate::server::Result;
use crate::server::api::diagnostics::publish_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
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
        let Ok(key) = session.key_from_url(uri.clone()) else {
            tracing::debug!("Failed to create document key from URI: {}", uri);
            return Ok(());
        };

        let document = TextDocument::new(text, version).with_language_id(&language_id);
        session.open_text_document(key.path(), document);

        match key.path() {
            AnySystemPath::System(system_path) => {
                let db = match session.project_db_for_path_mut(system_path) {
                    Some(db) => db,
                    None => session.default_project_db_mut(),
                };
                let Ok(file) = system_path_to_file(db, system_path) else {
                    // This can only fail when the path is a directory or it doesn't exists but the
                    // file should exists for this handler in this branch.
                    tracing::warn!("Failed to create a salsa file for {}", system_path);
                    return Ok(());
                };
                db.project().open_file(db, file);
                // TODO: Why do we require this? Because this doesn't do anything as `File` doesn't
                // exists for the system path and this event will not create it either.
                db.apply_changes(vec![ChangeEvent::Opened(system_path.clone())], None);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.default_project_db_mut();
                let virtual_file = db.files().virtual_file(db, virtual_path);
                db.project().open_file(db, virtual_file.file());
            }
        }

        publish_diagnostics(session, &key, client)
    }
}
