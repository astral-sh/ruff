use lsp_types::notification::DidOpenTextDocument;
use lsp_types::DidOpenTextDocumentParams;

use red_knot_workspace::watch::ChangeEvent;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::system::url_to_system_path;
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
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        let Ok(path) = url_to_system_path(&params.text_document.uri) else {
            return Ok(());
        };

        let document = TextDocument::new(params.text_document.text, params.text_document.version);
        session.open_text_document(params.text_document.uri, document);

        let db = match session.workspace_db_for_path_mut(path.as_std_path()) {
            Some(db) => db,
            None => session.default_workspace_db_mut(),
        };
        db.apply_changes(vec![ChangeEvent::file_created(path)], None);

        // TODO(dhruvmanila): Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
