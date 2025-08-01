use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};
use ruff_db::Db as _;
use ruff_db::files::system_path_to_file;
use ty_project::Db as _;
use ty_project::watch::{ChangeEvent, CreatedKind};

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

        // This is a "maybe" because the `File` might've not been interned yet i.e., the
        // `try_system` call will return `None` which doesn't mean that the file is new, it's just
        // that the server didn't need the file yet.
        let is_maybe_new_system_file = path.as_system().is_some_and(|system_path| {
            let db = session.project_db(path);
            db.files()
                .try_system(db, system_path)
                .is_none_or(|file| !file.exists(db))
        });

        match path {
            AnySystemPath::System(system_path) => {
                let event = if is_maybe_new_system_file {
                    ChangeEvent::Created {
                        path: system_path.clone(),
                        kind: CreatedKind::File,
                    }
                } else {
                    ChangeEvent::Opened(system_path.clone())
                };
                session.apply_changes(path, vec![event]);

                let db = session.project_db_mut(path);
                match system_path_to_file(db, system_path) {
                    Ok(file) => db.project().open_file(db, file),
                    Err(err) => tracing::warn!("Failed to open file {system_path}: {err}"),
                }
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.project_db_mut(path);
                let virtual_file = db.files().virtual_file(db, virtual_path);
                db.project().open_file(db, virtual_file.file());
            }
        }

        publish_diagnostics(session, &key, client);

        Ok(())
    }
}
