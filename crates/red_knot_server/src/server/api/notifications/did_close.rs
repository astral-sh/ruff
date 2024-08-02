use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::DidCloseTextDocumentParams;

use ruff_db::files::system_path_to_file;

use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::url_to_system_path;

pub(crate) struct DidCloseTextDocumentHandler;

impl NotificationHandler for DidCloseTextDocumentHandler {
    type NotificationType = DidCloseTextDocument;
}

impl SyncNotificationHandler for DidCloseTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: DidCloseTextDocumentParams,
    ) -> Result<()> {
        let Ok(path) = url_to_system_path(&params.text_document.uri) else {
            return Ok(());
        };

        let key = session.key_from_url(params.text_document.uri);
        session
            .close_document(&key)
            .with_failure_code(ErrorCode::InternalError)?;

        if let Some(db) = session.workspace_db_for_path_mut(path.as_std_path()) {
            let file = system_path_to_file(&**db, path).unwrap();
            db.workspace().close_file(db.get_mut(), file);
        }

        clear_diagnostics(key.url(), &notifier)?;

        Ok(())
    }
}
