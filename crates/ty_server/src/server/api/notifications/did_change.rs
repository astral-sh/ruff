use lsp_server::ErrorCode;
use lsp_types::notification::DidChangeTextDocument;
use lsp_types::DidChangeTextDocumentParams;

use ty_project::watch::ChangeEvent;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::{url_to_any_system_path, AnySystemPath};

pub(crate) struct DidChangeTextDocumentHandler;

impl NotificationHandler for DidChangeTextDocumentHandler {
    type NotificationType = DidChangeTextDocument;
}

impl SyncNotificationHandler for DidChangeTextDocumentHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DidChangeTextDocumentParams,
    ) -> Result<()> {
        let Ok(path) = url_to_any_system_path(&params.text_document.uri) else {
            return Ok(());
        };

        let key = session.key_from_url(params.text_document.uri);

        session
            .update_text_document(&key, params.content_changes, params.text_document.version)
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

        // TODO(dhruvmanila): Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
