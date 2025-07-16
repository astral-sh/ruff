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

        let path = key.path();
        let db = session.project_db_mut(path);

        match path {
            AnySystemPath::System(system_path) => {
                match system_path_to_file(db, system_path) {
                    Ok(file) => db.project().open_file(db, file),
                    Err(err) => {
                        // This can only fail when the path is a directory or it doesn't exists but
                        // the file should exists for this handler in this branch because it was
                        // added to the `Index` (using `open_text_document` above) and the
                        // `LSPSystem` should return it when reading it from the index.
                        tracing::warn!("Failed to create a salsa file for {system_path}: {err}");
                    }
                }
                session.apply_changes(path, vec![ChangeEvent::Opened(system_path.clone())]);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let virtual_file = db.files().virtual_file(db, virtual_path);
                db.project().open_file(db, virtual_file.file());
            }
        }

        publish_diagnostics(session, &key, client);

        Ok(())
    }
}
