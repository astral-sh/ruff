use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};

use ruff_db::Db;
use ty_project::watch::ChangeEvent;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::{url_to_any_system_path, AnySystemPath};
use crate::TextDocument;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
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
        session.open_text_document(uri, document);

        match path {
            AnySystemPath::System(path) => {
                let db = match session.project_db_for_path_mut(path.as_std_path()) {
                    Some(db) => db,
                    None => session.default_project_db_mut(),
                };
                db.apply_changes(vec![ChangeEvent::Opened(path)], None);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.default_project_db_mut();
                db.files().virtual_file(db, &virtual_path);
            }
        }

        // TODO(dhruvmanila): Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
