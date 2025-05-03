use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::DidCloseTextDocumentParams;
use ty_project::watch::ChangeEvent;

use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::{url_to_any_system_path, AnySystemPath};

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
        let Ok(path) = url_to_any_system_path(&params.text_document.uri) else {
            return Ok(());
        };

        let key = session.key_from_url(params.text_document.uri);
        session
            .close_document(&key)
            .with_failure_code(ErrorCode::InternalError)?;

        if let AnySystemPath::SystemVirtual(virtual_path) = path {
            let db = session.default_project_db_mut();
            db.apply_changes(vec![ChangeEvent::DeletedVirtual(virtual_path)], None);
        }

        clear_diagnostics(key.url(), &notifier)?;

        Ok(())
    }
}
